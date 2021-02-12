use rustls::{Certificate, NoClientAuth, PrivateKey};
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// FTPSConfig shows how TLS security is configured for the server or a particular channel.
#[derive(Clone, Debug)]
pub enum FTPSConfig {
    Off,
    On { certs_file: PathBuf, key_file: PathBuf },
}

#[derive(Debug, Copy, Clone)]
pub struct FTPSNotAvailable;

impl fmt::Display for FTPSNotAvailable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FTPS not configured/available")
    }
}

impl Error for FTPSNotAvailable {}

// Converts
impl TryFrom<FTPSConfig> for tokio_rustls::TlsAcceptor {
    type Error = FTPSNotAvailable;

    fn try_from(config: FTPSConfig) -> Result<Self, Self::Error> {
        match config {
            FTPSConfig::Off => Err(FTPSNotAvailable),
            FTPSConfig::On { certs_file, key_file } => {
                let acceptor: tokio_rustls::TlsAcceptor = new_config(certs_file, key_file).into();
                Ok(acceptor)
            }
        }
    }
}

pub fn new_config<P: AsRef<Path>>(certs_file: P, key_file: P) -> Arc<rustls::ServerConfig> {
    let certs: Vec<Certificate> = load_certs(certs_file);
    let privkey: PrivateKey = load_private_key(key_file);

    let mut config = rustls::ServerConfig::new(NoClientAuth::new());
    config.session_storage = Arc::new(rustls::NoServerSessionStorage {});
    config.key_log = Arc::new(rustls::KeyLogFile::new());
    config.set_single_cert(certs, privkey).expect("Failed to setup TLS certificate chain and key");
    Arc::new(config)
}

fn load_certs<P: AsRef<Path>>(filename: P) -> Vec<rustls::Certificate> {
    let certfile: File = File::open(filename).expect("cannot open certificate file");
    let mut reader: BufReader<File> = BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader).unwrap()
}

fn load_private_key<P: AsRef<Path>>(filename: P) -> rustls::PrivateKey {
    let rsa_keys = {
        let keyfile = File::open(&filename).expect("cannot open private key file");
        let mut reader = BufReader::new(keyfile);
        rustls::internal::pemfile::rsa_private_keys(&mut reader).expect("file contains invalid rsa private key")
    };

    let pkcs8_keys = {
        let keyfile = File::open(&filename).expect("cannot open private key file");
        let mut reader = BufReader::new(keyfile);
        rustls::internal::pemfile::pkcs8_private_keys(&mut reader).expect("file contains invalid pkcs8 private key (encrypted keys not supported)")
    };

    // prefer to load pkcs8 keys
    if !pkcs8_keys.is_empty() {
        pkcs8_keys[0].clone()
    } else {
        assert!(!rsa_keys.is_empty());
        rsa_keys[0].clone()
    }
}
