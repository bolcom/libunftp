#![allow(missing_docs)]

//! Support for tests of FTPS: a certificate made for the test, and a TLS client that trusts it.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A certificate and key in files (as `ServerBuilder::ftps` takes them), and a client that trusts the certificate. The files are
/// removed when this is dropped.
pub struct Tls {
    pub certs: PathBuf,
    pub key: PathBuf,
    pub connector: TlsConnector,
    dir: PathBuf,
}

impl Tls {
    pub fn new() -> Tls {
        let dir = std::env::temp_dir().join(format!("libunftp_ftps_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let (certs, key) = (dir.join("server.certs"), dir.join("server.key"));
        std::fs::write(&certs, certified.cert.pem()).unwrap();
        std::fs::write(&key, certified.signing_key.serialize_pem()).unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(certified.cert.der().clone()).unwrap();
        let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        Tls {
            certs,
            key,
            connector: TlsConnector::from(Arc::new(config)),
            dir,
        }
    }

    /// The name the certificate is made for
    pub fn server_name() -> ServerName<'static> {
        ServerName::try_from("localhost").unwrap()
    }
}

impl Default for Tls {
    fn default() -> Self {
        Tls::new()
    }
}

impl Drop for Tls {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
