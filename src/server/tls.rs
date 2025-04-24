use crate::options::{FtpsClientAuth, TlsFlags};
use rustls::{
    crypto::{aws_lc_rs, aws_lc_rs::Ticketer},
    pki_types::{CertificateDer, PrivateKeyDer},
    server::{ClientCertVerifierBuilder, NoServerSessionStorage, StoresServerSessions, WebPkiClientVerifier},
    version::{TLS12, TLS13},
    NoKeyLog, RootCertStore, ServerConfig, SupportedProtocolVersion,
};
use std::{
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

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

impl Display for FtpsNotAvailable {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "FTPS not configured/available")
    }
}

impl std::error::Error for FtpsNotAvailable {}

// The error returned by new_config
#[derive(Error, Debug)]
#[error("TLS configuration error")]
pub enum ConfigError {
    #[error("found no private key")]
    NoPrivateKey,

    #[error("error reading key/cert input")]
    Load(#[from] io::Error),

    #[error("error building root certs")]
    RootCerts(rustls::Error),

    #[error("error initialising Rustls")]
    RustlsInit(#[from] rustls::Error),

    #[error("error initialising the client cert verifier")]
    ClientVerifier(#[from] rustls::server::VerifierBuilderError),
}

pub fn new_config<P: AsRef<Path>>(
    certs_file: P,
    key_file: P,
    flags: TlsFlags,
    client_auth: FtpsClientAuth,
    trust_store: P,
) -> Result<Arc<ServerConfig>, ConfigError> {
    let certs: Vec<CertificateDer> = load_certs(certs_file)?;
    let privkey: PrivateKeyDer = load_private_key(key_file)?;

    let client_auther = match client_auth {
        FtpsClientAuth::Off => Ok(WebPkiClientVerifier::no_client_auth()),
        FtpsClientAuth::Request => {
            let builder: ClientCertVerifierBuilder = WebPkiClientVerifier::builder(Arc::new(root_cert_store(trust_store)?));
            builder.allow_unauthenticated().build()
        }
        FtpsClientAuth::Require => {
            let builder: ClientCertVerifierBuilder = WebPkiClientVerifier::builder(Arc::new(root_cert_store(trust_store)?));
            builder.build()
        }
    }
    .map_err(ConfigError::ClientVerifier)?;

    let mut versions: Vec<&SupportedProtocolVersion> = vec![];
    if flags.contains(TlsFlags::V1_2) {
        versions.push(&TLS12)
    }
    if flags.contains(TlsFlags::V1_3) {
        versions.push(&TLS13)
    }

    let provider = Arc::new(aws_lc_rs::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&versions)
        .map_err(ConfigError::RustlsInit)?
        .with_client_cert_verifier(client_auther)
        .with_single_cert(certs, privkey)
        .map_err(ConfigError::RustlsInit)?; // No SNI, single certificate

    // Support session resumption with server side state (Session IDs)
    config.session_storage = if flags.contains(TlsFlags::RESUMPTION_SESS_ID) {
        TlsSessionCache::new(1024)
    } else {
        Arc::new(NoServerSessionStorage {})
    };
    // Support session resumption with tickets. See https://tools.ietf.org/html/rfc5077
    if flags.contains(TlsFlags::RESUMPTION_TICKETS) {
        config.ticketer = Ticketer::new().map_err(ConfigError::RustlsInit)?;
    };
    // Don't allow dumping session keys
    config.key_log = Arc::new(NoKeyLog {});

    Ok(Arc::new(config))
}

fn root_cert_store<P: AsRef<Path>>(trust_pem: P) -> Result<RootCertStore, ConfigError> {
    let mut store = RootCertStore::empty();
    let certs = load_certs(trust_pem)?;
    for cert in certs.iter() {
        store.add(cert.clone()).map_err(ConfigError::RootCerts)?
    }
    Ok(store)
}

fn load_certs<P: AsRef<Path>>(filename: P) -> Result<Vec<CertificateDer<'static>>, ConfigError> {
    let certfile: File = File::open(filename)?;
    let mut reader: BufReader<File> = BufReader::new(certfile);
    let certs = rustls_pemfile::certs(&mut reader);
    let mut res = Vec::new();
    for cert in certs {
        let cert = cert.map_err(ConfigError::Load)?;
        res.push(cert);
    }

    Ok(res)
}

fn load_private_key<P: AsRef<Path>>(filename: P) -> Result<PrivateKeyDer<'static>, ConfigError> {
    use rustls_pemfile::{read_one, Item};
    use std::iter;

    let keyfile = File::open(&filename)?;
    let mut reader = BufReader::new(keyfile);

    for item in iter::from_fn(|| read_one(&mut reader).transpose()) {
        match item {
            Ok(Item::Pkcs1Key(key)) => return Ok(PrivateKeyDer::Pkcs1(key)),
            Ok(Item::Pkcs8Key(key)) => return Ok(PrivateKeyDer::Pkcs8(key)),
            Ok(Item::Sec1Key(key)) => return Ok(PrivateKeyDer::Sec1(key)),
            Err(e) => return Err(ConfigError::Load(e)),
            _ => {}
        }
    }

    Err(ConfigError::NoPrivateKey)
}

/// Stores the session IDs server side.
#[derive(Debug)]
struct TlsSessionCache {
    cache: moka::sync::Cache<Vec<u8>, Vec<u8>>,
}

impl TlsSessionCache {
    /// Make a new TlsSessionCache.  `size` is the maximum
    /// number of stored sessions.
    pub fn new(size: u64) -> Arc<TlsSessionCache> {
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
