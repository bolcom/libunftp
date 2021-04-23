use crate::options::TlsFlags;
use rustls::{internal::pemfile, Certificate, NoClientAuth, NoServerSessionStorage, PrivateKey, ProtocolVersion, ServerConfig, Ticketer};
use std::error::Error;
use std::fmt;
use std::fmt::Formatter;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

pub fn new_config<P: AsRef<Path>>(certs_file: P, key_file: P, flags: TlsFlags) -> std::io::Result<Arc<ServerConfig>> {
    let certs: Vec<Certificate> = load_certs(certs_file)?;
    let privkey: PrivateKey = load_private_key(key_file)?;

    let mut config = rustls::ServerConfig::new(NoClientAuth::new());
    // Support session resumption with server side state (Session IDs)
    config.session_storage = if flags.contains(TlsFlags::RESUMPTION_SESS_ID) {
        TlsSessionCache::new(1024)
    } else {
        Arc::new(NoServerSessionStorage {})
    };
    // Support session resumption with tickets. See https://tools.ietf.org/html/rfc5077
    if flags.contains(TlsFlags::RESUMPTION_TICKETS) {
        config.ticketer = Ticketer::new();
    };
    // Don't allow dumping session keys
    config.key_log = Arc::new(rustls::NoKeyLog {});
    // No SNI, single certificate
    config
        .set_single_cert(certs, privkey)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let mut versions: Vec<ProtocolVersion> = vec![];
    if flags.contains(TlsFlags::V1_2) {
        versions.push(ProtocolVersion::TLSv1_2)
    }
    if flags.contains(TlsFlags::V1_3) {
        versions.push(ProtocolVersion::TLSv1_3)
    }
    config.versions = versions;

    Ok(Arc::new(config))
}

fn load_certs<P: AsRef<Path>>(filename: P) -> std::io::Result<Vec<Certificate>> {
    let certfile: File = File::open(filename)?;
    let mut reader: BufReader<File> = BufReader::new(certfile);
    pemfile::certs(&mut reader).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
}

fn load_private_key<P: AsRef<Path>>(filename: P) -> std::io::Result<PrivateKey> {
    let rsa_keys = {
        let keyfile = File::open(&filename)?;
        let mut reader = BufReader::new(keyfile);
        pemfile::rsa_private_keys(&mut reader).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?
    };

    let pkcs8_keys = {
        let keyfile = File::open(&filename)?;
        let mut reader = BufReader::new(keyfile);
        pemfile::pkcs8_private_keys(&mut reader).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?
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

    Ok(key)
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

impl rustls::StoresServerSessions for TlsSessionCache {
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
}
