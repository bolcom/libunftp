use std::io::BufReader;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use rustls;
use rustls::NoClientAuth;

pub fn new_config<P: AsRef<Path>>(certs_file: P, key_file: P) -> Arc<rustls::ServerConfig> {
    let certs = load_certs(certs_file);
    let privkey = load_private_key(key_file);

    let mut config = rustls::ServerConfig::new(NoClientAuth::new());
    config.key_log = Arc::new(rustls::KeyLogFile::new());
    config.set_single_cert(certs, privkey).expect("Failed to setup TLS certificate chain and key");
    Arc::new(config)
}

fn load_certs<P: AsRef<Path>>(filename: P) -> Vec<rustls::Certificate> {
    let certfile = File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
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
