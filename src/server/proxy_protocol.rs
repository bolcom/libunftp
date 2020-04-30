use super::session::SharedSession;
use crate::auth::UserDetail;
use crate::storage;

use bytes::Bytes;
use lazy_static::*;
use log::warn;
use proxy_protocol::version1::ProxyAddressFamily;
use proxy_protocol::ProxyHeader;
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::net::IpAddr;
use std::ops::Range;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;

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

/// Constructs a hash key based on the source ip and the destination port
/// in a straightforward consistent way
pub fn construct_proxy_hash_key(connection: &ConnectionTuple, port: u16) -> String {
    format!("{}.{}", connection.from_ip, port)
}

/// Connect clients to the right data channel
pub struct ProxyProtocolSwitchboard<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync,
    U: UserDetail,
{
    switchboard: HashMap<String, Option<SharedSession<S, U>>>,
    port_range: Range<u16>,
}

#[derive(Debug)]
pub enum ProxyProtocolError {
    // SwitchBoardNotInitialized,
    EntryNotAvailable,
    // EntryCreationFailed,
    MaxRetriesError,
}

impl<S, U> ProxyProtocolSwitchboard<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync,
    U: UserDetail + 'static,
{
    pub fn new(passive_ports: Range<u16>) -> Self {
        let board = HashMap::new();
        Self {
            switchboard: board,
            port_range: passive_ports,
        }
    }

    fn try_and_claim(&mut self, hash: String, session_arc: SharedSession<S, U>) -> Result<(), ProxyProtocolError> {
        match self.switchboard.get(&hash) {
            Some(_) => Err(ProxyProtocolError::EntryNotAvailable),
            None => match self.switchboard.insert(hash, Some(session_arc)) {
                Some(_) => {
                    warn!("This is a data race condition. This shouldn't happen");
                    // just return Ok anyway however
                    Ok(())
                }
                None => Ok(()),
            },
        }
    }

    fn get_hash_with_connection(connection: &ConnectionTuple) -> String {
        format!("{}.{}", connection.from_ip, connection.to_port)
    }

    pub fn unregister(&mut self, connection: &ConnectionTuple) {
        let hash = Self::get_hash_with_connection(connection);
        match self.switchboard.remove(&hash) {
            Some(_) => (),
            None => {
                warn!("Entry already removed?");
            }
        }
    }

    pub async fn get_session_by_incoming_data_connection(&mut self, connection: &ConnectionTuple) -> Option<SharedSession<S, U>> {
        let hash = Self::get_hash_with_connection(connection);

        match self.switchboard.get(&hash) {
            Some(session) => session.clone(),
            None => None,
        }
    }

    /// based on source ip of the client, select a free entry
    /// but initialize it to None
    // TODO: set a TTL on the hashmap entries
    pub async fn reserve_next_free_port(&mut self, session_arc: SharedSession<S, U>) -> Result<u16, ProxyProtocolError> {
        let rng_length = self.port_range.end - self.port_range.start;

        let mut rng = OS_RNG.lock().await;
        // change this to a "shuffle" method later on, to make sure we tried all available ports
        for _ in 1..10 {
            let port = rng.next_u32() % rng_length as u32 + self.port_range.start as u32;
            let session = session_arc.lock().await;
            if let Some(conn) = session.control_connection_info {
                let hash = construct_proxy_hash_key(&conn, port as u16);

                match &self.try_and_claim(hash.clone(), session_arc.clone()) {
                    Ok(_) => return Ok(port as u16),
                    Err(_) => continue,
                }
            }
        }
        // out of tries
        println!("Out of tries!");
        Err(ProxyProtocolError::MaxRetriesError)
    }
}

#[cfg(test)]
mod tests {
    use proxy_protocol::version1::ProxyAddressFamily;
    use proxy_protocol::ProxyHeader;
    use std::net::Shutdown;
    use std::net::{IpAddr::V4, Ipv4Addr};
    use tokio::io::AsyncWriteExt;

    async fn listen_server() -> tokio::net::TcpStream {
        let addr = "127.0.0.1:6142";
        let mut listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        listener.accept().await.unwrap().0
    }

    async fn connect_client() -> tokio::net::TcpStream {
        tokio::net::TcpStream::connect("127.0.0.1:6142").await.unwrap()
    }

    async fn get_connected_tcp_streams() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
        tokio::join!(listen_server(), connect_client())
    }

    #[tokio::test]
    async fn long_header() {
        let (mut s, mut c) = get_connected_tcp_streams().await;

        let server = tokio::spawn(async move { super::read_proxy_header(&mut s).await.unwrap() });
        let client = tokio::spawn(async move {
            c.write_all("PROXY TCP4 255.255.255.255 255.255.255.255 65535 65535\r\n".as_ref())
                .await
                .unwrap();
            c.shutdown(Shutdown::Both).unwrap();
        });

        let res = tokio::join!(server, client);

        assert_eq!(
            res.0.unwrap(),
            ProxyHeader::Version1 {
                family: ProxyAddressFamily::IPv4,
                source: V4(Ipv4Addr::new(255, 255, 255, 255)),
                destination: V4(Ipv4Addr::new(255, 255, 255, 255)),
                source_port: 65535,
                destination_port: 65535
            }
        );
    }
}
