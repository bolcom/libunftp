use crate::options::{FtpsClientAuth, TlsFlags};
use rustls::server::StoresServerSessions;
use rustls::{
    server::{AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, NoClientAuth, NoServerSessionStorage},
    version::{TLS12, TLS13},
    Certificate, NoKeyLog, PrivateKey, RootCertStore, ServerConfig, SupportedProtocolVersion, Ticketer,
};
use std::{
    error::Error,
    fmt,
    fmt::Formatter,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

// FTPSConfig shows how TLS security is configured for the server or a particular channel.
#[derive(Clone)]
pub enum FtpsConfig {
    Off,
    Building { certs_file: PathBuf, key_file: PathBuf },
    On { tls_config: Arc<ServerConfig> },
}

impl fmt::Debug for FtpsConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FtpsConfig::Off => write!(f, "Off"),
            FtpsConfig::Building { .. } => write!(f, "Building"),
            FtpsConfig::On { .. } => write!(f, "On"),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FtpsNotAvailable;

impl fmt::Display for FtpsNotAvailable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FTPS not configured/available")
    }
}

impl Error for FtpsNotAvailable {}

pub fn new_config<P: AsRef<Path>>(
    certs_file: P,
    key_file: P,
    flags: TlsFlags,
    client_auth: FtpsClientAuth,
    trust_store: P,
) -> std::io::Result<Arc<ServerConfig>> {
    let certs: Vec<Certificate> = load_certs(certs_file)?;
    let privkey: PrivateKey = load_private_key(key_file)?;

    let client_auther = match client_auth {
        FtpsClientAuth::Off => NoClientAuth::new(),
        FtpsClientAuth::Request => {
            let store: RootCertStore = root_cert_store(trust_store)?;
            AllowAnyAnonymousOrAuthenticatedClient::new(store)
        }
        FtpsClientAuth::Require => {
            let store: RootCertStore = root_cert_store(trust_store)?;
            AllowAnyAuthenticatedClient::new(store)
        }
    };

    let mut versions: Vec<&SupportedProtocolVersion> = vec![];
    if flags.contains(TlsFlags::V1_2) {
        versions.push(&TLS12)
    }
    if flags.contains(TlsFlags::V1_3) {
        versions.push(&TLS13)
    }

    let mut config = ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&versions).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .with_client_cert_verifier(client_auther)
        // No SNI, single certificate 
        .with_single_cert(certs, privkey).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Support session resumption with server side state (Session IDs)
    config.session_storage = if flags.contains(TlsFlags::RESUMPTION_SESS_ID) {
        TlsSessionCache::new(1024)
    } else {
        Arc::new(NoServerSessionStorage {})
    };
    // Support session resumption with tickets. See https://tools.ietf.org/html/rfc5077
    if flags.contains(TlsFlags::RESUMPTION_TICKETS) {
        config.ticketer = Ticketer::new().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    };
    // Don't allow dumping session keys
    config.key_log = Arc::new(NoKeyLog {});

    Ok(Arc::new(config))
}

fn root_cert_store<P: AsRef<Path>>(trust_pem: P) -> std::io::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    let certs = load_certs(trust_pem)?;
    for cert in certs.iter() {
        store.add(cert).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?
    }
    Ok(store)
}

fn load_certs<P: AsRef<Path>>(filename: P) -> std::io::Result<Vec<Certificate>> {
    let certfile: File = File::open(filename)?;
    let mut reader: BufReader<File> = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
        .map(|v| {
            let mut res = Vec::with_capacity(v.len());
            for e in v {
                res.push(Certificate(e));
            }
            res
        })
}

fn load_private_key<P: AsRef<Path>>(filename: P) -> std::io::Result<PrivateKey> {
    let rsa_keys = {
        let keyfile = File::open(&filename)?;
        let mut reader = BufReader::new(keyfile);
        rustls_pemfile::rsa_private_keys(&mut reader).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?
    };

    let pkcs8_keys = {
        let keyfile = File::open(&filename)?;
        let mut reader = BufReader::new(keyfile);
        rustls_pemfile::pkcs8_private_keys(&mut reader).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?
    };

    // prefer to load pkcs8 keys
    let key = if !pkcs8_keys.is_empty() {
        pkcs8_keys[0].clone()
    } else {
        if rsa_keys.is_empty() {
            return Err(std::io::Error::from(std::io::ErrorKind::Other));
        }
        rsa_keys[0].clone()
    };

    Ok(PrivateKey(key))
}

/// Stores the session IDs server side.
struct TlsSessionCache {
    cache: moka::sync::Cache<Vec<u8>, Vec<u8>>,
}

impl TlsSessionCache {
    /// Make a new TlsSessionCache.  `size` is the maximum
    /// number of stored sessions.
    pub fn new(size: usize) -> Arc<TlsSessionCache> {
        debug_assert!(size > 0);
        Arc::new(TlsSessionCache {
            cache: moka::sync::CacheBuilder::new(size).time_to_idle(Duration::from_secs(5 * 60)).build(),
        })
    }
}

impl StoresServerSessions for TlsSessionCache {
    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> bool {
        self.cache.insert(key, value);
        true
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.cache.get(&key.to_vec())
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        let key_as_vec = key.to_vec();
        self.cache.get(&key_as_vec)
        // For some reason rustls always calls take and so removes the session ID which then breaks
        // FileZilla for instance. So I implement take here to not really take, only get...
        // self.cache.invalidate(&key_as_vec);
    }

    fn can_cache(&self) -> bool {
        true
    }
}
