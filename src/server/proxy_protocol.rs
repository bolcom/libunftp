use super::session::SharedSession;
use crate::{auth::UserDetail, storage::StorageBackend};
use bytes::Bytes;
use lazy_static::lazy_static;
use proxy_protocol::{version1::ProxyAddressFamily, ProxyHeader};
use rand::{rngs::OsRng, RngCore};
use std::{collections::HashMap, net::IpAddr, ops::Range};
use tokio::{io::AsyncReadExt, sync::Mutex};

lazy_static! {
    static ref OS_RNG: Mutex<OsRng> = Mutex::new(OsRng);
}

#[derive(Clone, Copy, Debug)]
pub enum ProxyMode {
    Off,
    On { external_control_port: u16 },
}

impl From<u16> for ProxyMode {
    fn from(port: u16) -> Self {
        ProxyMode::On { external_control_port: port }
    }
}

#[derive(Debug, PartialEq)]
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

#[tracing_attributes::instrument]
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
                if i + pos > rbuf.capacity() || i + pos < 13 {
                    return Err(ProxyError::HeaderSize);
                }

                read_half.read(&mut rbuf[i..=i + pos]).await.unwrap();

                // make sure the message ends with crlf or it will panic
                if rbuf[i + pos - 1] != 0x0d {
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

#[tracing_attributes::instrument]
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
#[derive(Debug)]
pub struct ProxyProtocolSwitchboard<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail,
{
    switchboard: HashMap<String, Option<SharedSession<S, U>>>,
    port_range: Range<u16>,
    logger: slog::Logger,
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
    S: StorageBackend<U>,
    U: UserDetail + 'static,
{
    pub fn new(logger: slog::Logger, passive_ports: Range<u16>) -> Self {
        let board = HashMap::new();
        Self {
            switchboard: board,
            port_range: passive_ports,
            logger,
        }
    }

    fn try_and_claim(&mut self, hash: String, session_arc: SharedSession<S, U>) -> Result<(), ProxyProtocolError> {
        match self.switchboard.get(&hash) {
            Some(_) => Err(ProxyProtocolError::EntryNotAvailable),
            None => match self.switchboard.insert(hash, Some(session_arc)) {
                Some(_) => {
                    slog::warn!(self.logger, "This is a data race condition. This shouldn't happen");
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
                slog::warn!(self.logger, "Entry already removed?");
            }
        }
    }

    #[tracing_attributes::instrument]
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
    #[tracing_attributes::instrument]
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
        slog::warn!(self.logger, "Out of tries reserving next free port!");
        Err(ProxyProtocolError::MaxRetriesError)
    }
}

#[cfg(test)]
mod tests {
    use super::ProxyError;
    use proxy_protocol::version1::ProxyAddressFamily;
    use proxy_protocol::ProxyHeader;
    use std::net::Shutdown;
    use std::net::{IpAddr::V4, Ipv4Addr};
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::time::sleep;

    async fn listen_server(listener: tokio::net::TcpListener) -> tokio::net::TcpStream {
        listener.accept().await.unwrap().0
    }

    async fn connect_client(port: u16) -> tokio::net::TcpStream {
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap()
    }

    async fn get_connected_tcp_streams() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::join!(listen_server(listener), connect_client(port))
    }

    #[tokio::test]
    async fn long_header_parses_fine() {
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

    #[tokio::test]
    async fn bad_crlf_throws_error() {
        let (mut s, mut c) = get_connected_tcp_streams().await;

        let server = tokio::spawn(async move { super::read_proxy_header(&mut s).await });
        let client = tokio::spawn(async move {
            c.write_all("PROXY TCP4 255.255.255.255 255.255.255.255 65535 65535\n".as_ref()).await.unwrap();
            c.shutdown(Shutdown::Both).unwrap();
        });

        let res = tokio::join!(server, client);
        let res = res.0.unwrap();

        assert_eq!(res, Err(ProxyError::CrlfError));
    }

    #[tokio::test]
    async fn in_pieces_parses_fine() {
        let (mut s, mut c) = get_connected_tcp_streams().await;
        c.set_nodelay(true).unwrap();

        let server = tokio::spawn(async move { super::read_proxy_header(&mut s).await });
        let client = tokio::spawn(async move {
            c.write_all("PROXY TCP4 255.255.255.255 255.255.255.255 65535 65535".as_ref()).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            c.write_all("\r\n".as_ref()).await.unwrap();
            c.shutdown(Shutdown::Both).unwrap();
        });

        let res = tokio::join!(server, client);

        assert_eq!(
            res.0.unwrap(),
            Ok(ProxyHeader::Version1 {
                family: ProxyAddressFamily::IPv4,
                source: V4(Ipv4Addr::new(255, 255, 255, 255)),
                destination: V4(Ipv4Addr::new(255, 255, 255, 255)),
                source_port: 65535,
                destination_port: 65535
            })
        );
    }
}
