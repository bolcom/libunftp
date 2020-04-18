use bytes::Bytes;
use proxy_protocol::version1::ProxyAddressFamily;
use proxy_protocol::ProxyHeader;
use std::net::IpAddr;
use tokio::io::AsyncReadExt;
use futures::channel::mpsc::{channel, Receiver, Sender};
use chashmap::CHashMap;
use std::ops::Range;
use rand::rngs::OsRng;
use rand::RngCore;
use lazy_static::*;
use tokio::sync::Mutex;
use log::warn;
lazy_static! {
    static ref OS_RNG: Mutex<OsRng> = Mutex::new(OsRng);
}

#[derive(Debug)]
pub enum ProxyError {
    CrlfError,
    HeaderSize,
    NotProxyHdr,
    DecodeError,
    IPv4Required,
    UnsupportedVersion,
}

#[derive(Debug, Copy, Clone)]
pub struct ConnectionTuple {
    pub from_ip: IpAddr,
    pub from_port: u16,
    pub to_ip: IpAddr,
    pub to_port: u16,
}

impl ConnectionTuple {
    fn new(from_ip: IpAddr, from_port: u16, to_ip: IpAddr, to_port: u16) -> Self {
        ConnectionTuple {
            from_ip,
            from_port,
            to_ip,
            to_port,
        }
    }
}

async fn read_proxy_header(tcp_stream: &mut tokio::net::TcpStream) -> Result<ProxyHeader, ProxyError> {
    let mut pbuf = vec![0; 108];
    let mut rbuf = vec![0; 108];
    let (mut read_half, _) = tcp_stream.split();
    let mut i = 0;

    loop {
        let n = read_half.peek(&mut pbuf).await.unwrap();
        match pbuf.iter().position(|b| *b == b'\n') {
            Some(pos) => {
                // invalid header size
                if i + pos > rbuf.capacity() || pos < 13 {
                    return Err(ProxyError::HeaderSize);
                }

                read_half.read(&mut rbuf[i..=i + pos]).await.unwrap();

                // make sure the message ends with crlf or it will panic
                if rbuf[pos - 1] != 0x0d {
                    return Err(ProxyError::CrlfError);
                }

                let mut phb = Bytes::copy_from_slice(&rbuf[..=i + pos]);
                let proxyhdr = match ProxyHeader::decode(&mut phb) {
                    Ok(h) => h,
                    Err(_) => return Err(ProxyError::DecodeError),
                };

                return Ok(proxyhdr);
            }
            None => {
                if i + n > rbuf.capacity() {
                    return Err(ProxyError::NotProxyHdr);
                }

                read_half.read(&mut rbuf[i..i + n]).await.unwrap();
                i += n;
            }
        }
    }
}

pub async fn get_peer_from_proxy_header(tcp_stream: &mut tokio::net::TcpStream) -> Result<ConnectionTuple, ProxyError> {
    let proxyhdr = match read_proxy_header(tcp_stream).await {
        Ok(v) => v,
        Err(e) => {
            return Err(e);
        }
    };
    match proxyhdr {
        ProxyHeader::Version1 {
            family,
            source,
            source_port,
            destination,
            destination_port,
            ..
        } => {
            if family == ProxyAddressFamily::IPv4 {
                Ok(ConnectionTuple::new(source, source_port, destination, destination_port))
            } else {
                Err(ProxyError::IPv4Required)
            }
        }
        _ => Err(ProxyError::UnsupportedVersion),
    }
}

#[derive(Debug)]
pub enum ProxyProtocolMsg {
    /// to be responded
    PassivePort(std::net::SocketAddr),
    /// TcpStream
    TcpStream(tokio::net::TcpStream),
}

#[derive(Debug)]
pub enum ProxyProtocolCallback {
    /// Command to assign a data port based on data from the
    /// ConnectionTuple (namely: source_ip) and the unique (control)
    /// session is identified by the entire ConnectionTuple
    AssignDataPortCommand(ConnectionTuple),
}

pub struct ProxyDataChannel {
    tx: Sender<ProxyProtocolMsg>,
    rx: Receiver<ProxyProtocolMsg>,
    key: Option<String>,
}

impl ProxyDataChannel {
    pub fn new(tx: Sender<ProxyProtocolMsg>, rx: Receiver<ProxyProtocolMsg>) -> Self {
        Self { tx, rx, key: None }
    }
}

/// Constructs a hash key based on the source ip and the destination port
/// in a straightforward consistent way
pub fn construct_proxy_hash_key(connection: &ConnectionTuple, port: u16) -> String {
    format!("{}.{}", connection.from_ip, port)
}

/// Connect clients to the right data channel
#[derive(Debug)]
pub struct ProxyProtocolSwitchboard {
    switchboard: CHashMap<String, Option<Sender<ProxyProtocolMsg>>>,
}

#[derive(Debug)]
pub enum ProxyProtocolError {
    SwitchBoardNotInitialized,
    EntryNotAvailable,
    EntryCreationFailed,
    MaxRetriesError,
}

impl ProxyProtocolSwitchboard {
    pub fn new() -> Self {
        let board = CHashMap::new();
        Self {
            switchboard: board,
        }
    }

    fn try_and_claim(&self, hash: String) -> Result<(),ProxyProtocolError> {
        match self.switchboard.get(&hash) {
            Some(_) => Err(ProxyProtocolError::EntryNotAvailable),
            None => match self.switchboard.insert(hash, None) {
                Some(_) => {
                    warn!("This is a data race condition. This shouldn't happen");
                    // just return Ok anyway however
                    Ok(())
                }
                None => {
                    Ok(())
                },
            }
        }
    }

    /// based on source ip of the client, select a free entry
    /// but initialize it to None
    pub async fn reserve_next_free_port(self, channel: &ProxyDataChannel, conn: &ConnectionTuple, port_range: Range<u16>) -> Result<String, ProxyProtocolError> {
        let rng_length = port_range.end - port_range.start;

        let mut rng = OS_RNG.lock().await;
        // change this to a "shuffle" method later on, to make sure we tried all available ports
        for _ in 1..10 {
            let port = rng.next_u32() % rng_length as u32 + port_range.start as u32;
            let hash = construct_proxy_hash_key(conn, port as u16);
            match &self.try_and_claim(hash.clone()) {
                Ok(_) => return Ok(hash),
                Err(_) => continue,
            }
        }
        // out of tries
        println!("Out of tries!");
        Err(ProxyProtocolError::MaxRetriesError)
    }
}
