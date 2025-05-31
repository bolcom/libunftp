#![cfg_attr(not(feature = "proxy_protocol"), allow(dead_code, unused_imports))]
use crate::server::switchboard::SocketAddrPair;
use bytes::Bytes;
#[cfg(feature = "proxy_protocol")]
use proxy_protocol::{ParseError, ProxyHeader, parse, version1::ProxyAddresses};
use std::net::{SocketAddr, SocketAddrV4};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;

#[derive(Error, Debug)]
#[error("Proxy Protocol Error")]
enum ProxyError {
    #[error("header doesn't end with CRLF")]
    CrlfError,
    #[error("header size is incorrect")]
    HeaderSize,
    #[error("header does not match the supported proxy protocol v1")]
    NotProxyHdr,
    #[cfg(feature = "proxy_protocol")]
    #[error("proxy protocol parse error")]
    DecodeError(#[from] ParseError),
    #[error("only IPv4 is supported")]
    IPv4Required,
    #[error("unsupported proxy protocol version")]
    UnsupportedVersion,
    #[error("error reading from stream")]
    ReadError(#[from] std::io::Error),
}

impl PartialEq for ProxyError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

/// Read the PROXY protocol v1 header from the provided TCP stream.
///
/// This function reads the header until it finds a line ending with a CR-LF (carriage return and line feed) sequence.
/// It then parses the header and returns the resulting `ProxyHeader` struct, which contains information about the connection's
/// source and destination IP addresses, source and destination ports and protocol family.
///
/// If the header size is invalid, or the header does not end with a CR-LF sequence, the function returns a `ProxyError`
/// with the reason for the failure. If there is a problem reading from the TCP stream, the function returns a `ProxyError::ReadError`.
/// If the header cannot be parsed, the function returns a `ProxyError::DecodeError`.
#[cfg(feature = "proxy_protocol")]
#[tracing_attributes::instrument]
async fn read_proxy_header(tcp_stream: &mut tokio::net::TcpStream) -> Result<ProxyHeader, ProxyError> {
    // Create two vectors to hold the data read from the TCP stream
    let mut pbuf = vec![0; 108]; // peek buffer
    let mut rbuf = vec![0; 108]; // read buffer

    let mut i = 0;

    loop {
        // Peek at the next data in the stream and map the error to a `ProxyError`
        let n = tcp_stream.peek(&mut pbuf).await.map_err(ProxyError::ReadError)?;

        match pbuf.iter().position(|b| *b == b'\n') {
            // If a newline character is found, the proxy header should be complete
            Some(pos) => {
                // If the header size is invalid, return an error
                if i + pos > rbuf.capacity() || i + pos < 13 {
                    return Err(ProxyError::HeaderSize);
                }

                // Read the data from the stream into the read buffer and map the error to a `ProxyError`
                tcp_stream.read(&mut rbuf[i..=i + pos]).await.map_err(ProxyError::ReadError)?;

                // Make sure the message ends with a CR/LF (\r\n)
                if rbuf[i + pos - 1] != 0x0d {
                    return Err(ProxyError::CrlfError);
                }

                // Create a byte array from the read buffer and parse it into a `ProxyHeader`
                let mut phb = Bytes::copy_from_slice(&rbuf[..=i + pos]);
                let proxyhdr = parse(&mut phb).map_err(ProxyError::DecodeError)?;

                return Ok(proxyhdr);
            }
            // If no newline character is found yet
            None => {
                // If the read buffer is full, return an error
                if i + n > rbuf.capacity() {
                    return Err(ProxyError::NotProxyHdr);
                }

                // Read the data that's available from the stream into the read buffer and map the error to a `ProxyError`
                i += tcp_stream.read(&mut rbuf[i..i + n]).await.map_err(ProxyError::ReadError)?;
            }
        }
    }
}

/// Takes a tcp stream and reads the proxy protocol header
/// Sends the extracted proxy connection information (source ip+port, destination ip+port) to the proxy loop
#[cfg(feature = "proxy_protocol")]
#[tracing_attributes::instrument]
pub(super) fn spawn_proxy_header_parsing(logger: slog::Logger, mut tcp_stream: tokio::net::TcpStream, tx: Sender<ProxyHeaderReceived>) {
    tokio::spawn(async move {
        match read_proxy_header(&mut tcp_stream).await {
            Ok(ProxyHeader::Version1 {
                addresses: ProxyAddresses::Ipv4 { source, destination },
            }) => {
                if let Err(e) = tx
                    .send(ProxyHeaderReceived(
                        SocketAddrPair {
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

/// Upon receiving the header, the connection and tcp stream are passed back to the proxy loop
pub(crate) struct ProxyHeaderReceived(pub SocketAddrPair, pub TcpStream);

#[cfg(test)]
mod tests {
    use super::ProxyError;
    use proxy_protocol::{ProxyHeader, version1::ProxyAddresses};
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
