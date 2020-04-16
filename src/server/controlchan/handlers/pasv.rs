//! The RFC 959 Passive (`PASV`) command
//
// This command requests the server-DTP to "listen" on a data
// port (which is not its default data port) and to wait for a
// connection rather than initiate one upon receipt of a
// transfer command.  The response to this command includes the
// host and port address this server is listening on.

use super::handler::CommandContext;
use crate::server::controlchan::handlers::CommandHandler;
use crate::server::controlchan::Command;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;

use async_trait::async_trait;
use futures::channel::mpsc::{channel, Receiver, Sender};
use rand::rngs::OsRng;
use rand::RngCore;
use std::io;
use std::ops::Range;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use lazy_static::*;

const BIND_RETRIES: u8 = 10;
lazy_static! {
    static ref OS_RNG: Mutex<OsRng> = Mutex::new(OsRng);
}

pub struct Pasv {}

impl Pasv {
    pub fn new() -> Self {
        Pasv {}
    }

    async fn try_port_range(local_addr: std::net::SocketAddr, passive_addrs: Range<u16>) -> io::Result<TcpListener> {
        let rng_length = passive_addrs.end - passive_addrs.start;

        let mut listener: io::Result<TcpListener> = Err(io::Error::new(io::ErrorKind::InvalidInput, "Bind retries cannot be 0"));

        let mut rng = OS_RNG.lock().await;
        for _ in 1..BIND_RETRIES {
            let port = rng.next_u32() % rng_length as u32 + passive_addrs.start as u32;
            listener = TcpListener::bind(std::net::SocketAddr::new(local_addr.ip(), port as u16)).await;
            if listener.is_ok() {
                break;
            }
        }

        listener
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Pasv
where
    U: 'static + Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        // obtain the ip address the client is connected to
        let conn_addr = match args.local_addr {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
        };

        let listener = Pasv::try_port_range(args.local_addr, args.passive_ports).await;

        let mut listener = match listener {
            Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Ok(l) => l,
        };

        let addr = match listener.local_addr()? {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
        };

        let octets = conn_addr.ip().octets();
        let port = addr.port();
        let p1 = port >> 8;
        let p2 = port - (p1 * 256);
        let tx = args.tx.clone();

        let (cmd_tx, cmd_rx): (Sender<Command>, Receiver<Command>) = channel(1);
        let (data_abort_tx, data_abort_rx): (Sender<()>, Receiver<()>) = channel(1);

        {
            let mut session = args.session.lock().await;
            session.data_cmd_tx = Some(cmd_tx);
            session.data_cmd_rx = Some(cmd_rx);
            session.data_abort_tx = Some(data_abort_tx);
            session.data_abort_rx = Some(data_abort_rx);
        }

        let session = args.session.clone();

        // Open the data connection in a new task and process it.
        // We cannot await this since we first need to let the client know where to connect :-)
        tokio::spawn(async move {
            if let Ok((socket, _socket_addr)) = listener.accept().await {
                let tx = tx.clone();
                let session_arc = session.clone();
                let mut session = session_arc.lock().await;
                session.spawn_data_processing(socket, tx);
            }
        });

        Ok(Reply::new_with_string(
            ReplyCode::EnteringPassiveMode,
            format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
        ))
    }
}
