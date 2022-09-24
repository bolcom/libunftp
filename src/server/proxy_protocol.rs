use super::chancomms::ProxyLoopSender;
use super::session::SharedSession;
use crate::server::chancomms::ProxyLoopMsg;
use crate::{auth::UserDetail, storage::StorageBackend};
use bytes::Bytes;
use proxy_protocol::{parse, version1::ProxyAddresses, ProxyHeader};
use std::net::{SocketAddr, SocketAddrV4};
use std::{collections::HashMap, net::IpAddr, ops::Range};
use tokio::io::AsyncReadExt;

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

#[derive(Debug, PartialEq, Eq)]
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
    pub source: SocketAddr,
    pub destination: SocketAddr,
}

impl ConnectionTuple {
    pub fn key(&self) -> String {
        format!("{}.{}", self.source.ip(), self.destination.port())
    }
}

#[tracing_attributes::instrument]
async fn read_proxy_header(tcp_stream: &mut tokio::net::TcpStream) -> Result<ProxyHeader, ProxyError> {
    let mut pbuf = vec![0; 108];
    let mut rbuf = vec![0; 108];
    let (mut read_half, _) = tcp_stream.split();
    let mut i = 0;

    // TODO: We mute the clippy warning now but perhaps the 'read' invocations below should be changed
    //       to read_exact
    #[allow(clippy::unused_io_amount)]
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
                let proxyhdr = match parse(&mut phb) {
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

//#[tracing_attributes::instrument]
pub fn spawn_proxy_header_parsing<Storage, User>(logger: slog::Logger, mut tcp_stream: tokio::net::TcpStream, tx: ProxyLoopSender<Storage, User>)
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
{
    tokio::spawn(async move {
        match read_proxy_header(&mut tcp_stream).await {
            Ok(ProxyHeader::Version1 {
                addresses: ProxyAddresses::Ipv4 { source, destination },
            }) => {
                if let Err(e) = tx
                    .send(ProxyLoopMsg::ProxyHeaderReceived(
                        ConnectionTuple {
                            source: SocketAddr::V4(SocketAddrV4::new(*source.ip(), source.port())),
                            destination: SocketAddr::V4(SocketAddrV4::new(*destination.ip(), destination.port())),
                        },
                        tcp_stream,
                    ))
                    .await
                {
                    slog::warn!(logger, "proxy protocol unable to send to channel: {:?}", e)
                };
            }
            Ok(ProxyHeader::Version1 {
                addresses: ProxyAddresses::Ipv6 { .. },
            }) => {
                slog::warn!(logger, "proxy protocol decode error: {:?}", ProxyError::IPv4Required);
            }
            Ok(_) => {
                slog::warn!(logger, "proxy protocol decode error: {:?}", ProxyError::UnsupportedVersion);
            }
            Err(e) => {
                slog::warn!(logger, "proxy protocol read error: {:?}", e);
            }
        }
    });
}

/// Constructs a hash key based on the source ip and the destination port
/// in a straightforward consistent way
pub fn construct_proxy_hash_key(source: &IpAddr, port: u16) -> String {
    format!("{}.{}", source, port)
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

    pub fn unregister(&mut self, connection: &ConnectionTuple) {
        let hash = connection.key();
        match self.switchboard.remove(&hash) {
            Some(_) => (),
            None => {
                slog::warn!(self.logger, "Entry already removed?");
            }
        }
    }

    #[tracing_attributes::instrument]
    pub async fn get_session_by_incoming_data_connection(&mut self, connection: &ConnectionTuple) -> Option<SharedSession<S, U>> {
        let hash = connection.key();

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

        // change this to a "shuffle" method later on, to make sure we tried all available ports
        for _ in 1..10 {
            let random_u32 = {
                let mut data = [0; 4];
                getrandom::getrandom(&mut data).expect("Error generating random free port to reserve");
                u32::from_ne_bytes(data)
            };

            let port = random_u32 % rng_length as u32 + self.port_range.start as u32;
            let session = session_arc.lock().await;
            if session.destination.is_some() {
                let hash = construct_proxy_hash_key(&session.source.ip(), port as u16);

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
    use proxy_protocol::{version1::ProxyAddresses, ProxyHeader};
    use std::net::{Ipv4Addr, SocketAddrV4};
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
            c.shutdown().await.unwrap();
        });

        let res = tokio::join!(server, client);

        assert_eq!(
            res.0.unwrap(),
            ProxyHeader::Version1 {
                addresses: {
                    ProxyAddresses::Ipv4 {
                        source: SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535),
                        destination: SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535),
                    }
                }
            }
        );
    }

    #[tokio::test]
    async fn bad_crlf_throws_error() {
        let (mut s, mut c) = get_connected_tcp_streams().await;

        let server = tokio::spawn(async move { super::read_proxy_header(&mut s).await });
        let client = tokio::spawn(async move {
            c.write_all("PROXY TCP4 255.255.255.255 255.255.255.255 65535 65535\n".as_ref()).await.unwrap();
            c.shutdown().await.unwrap();
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
            c.shutdown().await.unwrap();
        });

        let res = tokio::join!(server, client);

        assert_eq!(
            res.0.unwrap(),
            Ok(ProxyHeader::Version1 {
                addresses: {
                    ProxyAddresses::Ipv4 {
                        source: SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535),
                        destination: SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535),
                    }
                }
            })
        );
    }
}
