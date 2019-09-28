use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use futures::Future;
use futures::{Async, Poll};
use log::debug;
use rustls;
use rustls::{NoClientAuth, Session};
use tokio::net::TcpStream;
use tokio_io::{AsyncRead, AsyncWrite};

#[derive(Debug, Clone)]
pub enum SecurityState {
    /// We're in TLS mode
    On,
    /// We're in plaintext mode
    Off,
}

pub trait SecuritySwitch {
    /// Tells if the specified channel is in secure mode or not.
    fn which_state(&self, channel: u8) -> SecurityState;
}

// A stream that can switch between TLS and plaintext mode depending on the state of provided
// SecuritySwitch.
#[derive(Debug)]
pub struct SwitchingTlsStream<S: SecuritySwitch> {
    tcp: TcpStream,
    tls: rustls::ServerSession,
    state: Arc<Mutex<S>>,
    channel: u8,
    eof: bool,
}

#[derive(Clone, Copy)]
enum TlsIoType {
    Read,
    Write,
}

impl<S: SecuritySwitch> SwitchingTlsStream<S> {
    pub fn new<P: AsRef<Path>>(delegate: TcpStream, switch: Arc<Mutex<S>>, channel: u8, certs_file: P, key_file: P) -> SwitchingTlsStream<S> {
        let certs = <SwitchingTlsStream<S>>::load_certs(certs_file);
        let privkey = <SwitchingTlsStream<S>>::load_private_key(key_file);

        let mut config = rustls::ServerConfig::new(NoClientAuth::new());
        config.key_log = Arc::new(rustls::KeyLogFile::new());
        config.set_single_cert(certs, privkey).expect("Failed to setup TLS certificate chain and key");
        let config = Arc::new(config);

        SwitchingTlsStream {
            tcp: delegate,
            state: switch,
            tls: rustls::ServerSession::new(&config),
            channel,
            eof: false,
        }
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

    fn tls_read_io(&mut self) -> io::Result<usize> {
        let len = self.tls.read_tls(&mut self.tcp)?;

        if let Err(e) = self.tls.process_new_packets() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
        }

        Ok(len)
    }

    fn tls_write_io(&mut self) -> io::Result<usize> {
        self.tls.write_tls(&mut self.tcp)
    }

    fn tls_io(&mut self, io_type: TlsIoType) -> io::Result<(usize, usize)> {
        let mut wrlen = 0;
        let mut rdlen = 0;

        let mut write_would_block = false;
        let mut read_would_block = false;

        while self.tls.wants_write() {
            match self.tls_write_io() {
                Ok(n) => wrlen += n,
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                    write_would_block = true;
                    break;
                }
                Err(err) => return Err(err),
            }
        }

        if !self.eof && self.tls.wants_read() {
            match self.tls_read_io() {
                Ok(0) => self.eof = true,
                Ok(n) => rdlen += n,
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => read_would_block = true,
                Err(err) => return Err(err),
            }
        }

        let would_block = match io_type {
            TlsIoType::Read => read_would_block,
            TlsIoType::Write => write_would_block,
        };

        if would_block {
            let would_block = match io_type {
                TlsIoType::Read => rdlen == 0,
                TlsIoType::Write => wrlen == 0,
            };

            return if would_block {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok((rdlen, wrlen))
            };
        }

        Ok((rdlen, wrlen))
    }
}

impl<S: SecuritySwitch> Read for SwitchingTlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let state = self.state.lock().unwrap().which_state(self.channel);
        match state {
            SecurityState::Off => self.tcp.read(buf),
            SecurityState::On => {
                if self.tls.is_handshaking() {
                    let mut handshake = Handshake {
                        tcp: &mut self.tcp,
                        tls: &mut self.tls,
                        channel: self.channel,
                    };

                    match handshake.poll() {
                        Result::Ok(Async::NotReady) => {
                            return Err(io::ErrorKind::WouldBlock.into());
                        }
                        Result::Err(e) => return Err(e),
                        Result::Ok(Async::Ready(_)) => {}
                    }
                }

                while self.tls.wants_read() {
                    if let (0, _) = self.tls_io(TlsIoType::Read)? {
                        break;
                    }
                }

                self.tls.read(buf)
            }
        }
    }
}

impl<S: SecuritySwitch> Write for SwitchingTlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let state = self.state.lock().unwrap().which_state(self.channel);
        match state {
            SecurityState::On => {
                if self.tls.is_handshaking() {
                    let mut handshake = Handshake {
                        tcp: &mut self.tcp,
                        tls: &mut self.tls,
                        channel: self.channel,
                    };

                    match handshake.poll() {
                        Result::Ok(Async::NotReady) => {
                            return Err(io::ErrorKind::WouldBlock.into());
                        }
                        Result::Err(e) => return Err(e),
                        Result::Ok(Async::Ready(_)) => {}
                    }
                }

                let len = self.tls.write(buf)?;

                while self.tls.wants_write() {
                    match self.tls_io(TlsIoType::Write) {
                        Ok(_) => (),
                        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock && len != 0 => break,
                        Err(err) => return Err(err),
                    }
                }

                if len != 0 || buf.is_empty() {
                    Ok(len)
                } else {
                    self.tls
                        .write(buf)
                        .and_then(|len| if len != 0 { Ok(len) } else { Err(io::ErrorKind::WouldBlock.into()) })
                }
            }
            SecurityState::Off => self.tcp.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let state = self.state.lock().unwrap().which_state(self.channel);
        match state {
            SecurityState::On => {
                self.tls.flush()?;
                while self.tls.wants_write() {
                    self.tls_io(TlsIoType::Write)?;
                }
                Ok(())
            }
            SecurityState::Off => self.tcp.flush(),
        }
    }
}

impl<S: SecuritySwitch> AsyncRead for SwitchingTlsStream<S> {}

impl<S: SecuritySwitch> AsyncWrite for SwitchingTlsStream<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        debug!("AsyncWrite shutdown <<{}>>", self.channel);
        // TODO: Anything to do for the TLS stream??
        AsyncWrite::shutdown(&mut self.tcp)
    }
}

struct Handshake<'a> {
    tcp: &'a mut TcpStream,
    tls: &'a mut rustls::ServerSession,
    channel: u8,
}

impl<'a> Future for Handshake<'a> {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        debug!("Performing handshake <<{}>>", self.channel);
        while self.tls.is_handshaking() {
            let rc = self.tls.read_tls(&mut self.tcp);
            if let Err(err) = rc {
                if io::ErrorKind::WouldBlock == err.kind() {
                    return Ok(Async::NotReady);
                }
                return Err(err);
            }

            if let Err(e) = self.tls.process_new_packets() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
            }

            while self.tls.wants_write() {
                if let Err(e) = self.tls.write_tls(&mut self.tcp) {
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
                }
            }
        }

        debug!("Handshake done <<{}>>", self.channel);
        Ok(Async::Ready(()))
    }
}
