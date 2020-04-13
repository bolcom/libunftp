use bytes::Bytes;
use proxy_protocol::version1::ProxyAddressFamily;
use proxy_protocol::ProxyHeader;
use std::net::IpAddr;
use tokio::io::AsyncReadExt;

#[derive(Debug)]
pub enum ProxyError {
    CrlfError,
    HeaderSize,
    NotProxyHdr,
    DecodeError,
    IPv4Required,
    UnsupportedVersion,
}

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
