//! The RFC 959 Passive (`PASSV`) command
//
// This command requests the server-DTP to "listen" on a data
// port (which is not its default data port) and to wait for a
// connection rather than initiate one upon receipt of a
// transfer command.  The response to this command includes the
// host and port address this server is listening on.

use crate::server::commands::{Cmd, Command};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::stream::Stream;
use rand::Rng;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const BIND_RETRIES: u8 = 10;

pub struct Pasv;

#[async_trait]
impl<S, U> Cmd<S, U> for Pasv
where
    U: 'static + Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        // obtain the ip address the client is connected to
        let conn_addr = match args.local_addr {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
        };

        //let mut rng = rand::thread_rng();
        // TODO: Re-enable this functionality somehow

        let mut listener: Option<std::net::TcpListener> = None;
        for _ in 1..BIND_RETRIES {
            //let i = rng.gen_range(0, args.passive_addrs.len() - 1);
            match std::net::TcpListener::bind(args.passive_addrs[0]) {
                Ok(x) => {
                    listener = Some(x);
                    break;
                }
                Err(_) => continue,
            };
        }

        let listener = match listener {
            None => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Some(l) => l,
        };

        let addr = match listener.local_addr()? {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
        };
        let listener = TcpListener::from_std(listener, &tokio::reactor::Handle::default())?;

        let octets = conn_addr.ip().octets();
        let port = addr.port();
        let p1 = port >> 8;
        let p2 = port - (p1 * 256);
        let tx = args.tx.clone();

        let (cmd_tx, cmd_rx): (mpsc::Sender<Command>, mpsc::Receiver<Command>) = mpsc::channel(1);
        let (data_abort_tx, data_abort_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);

        let mut session = args.session.lock().await;
        session.data_cmd_tx = Some(cmd_tx);
        session.data_cmd_rx = Some(cmd_rx);
        session.data_abort_tx = Some(data_abort_tx);
        session.data_abort_rx = Some(data_abort_rx);

        let session = args.session.clone();

        use futures03::compat::Stream01CompatExt;
        use futures03::StreamExt;
        use tokio::net::TcpListener;

        tokio02::spawn(async move {
            let mut strm = listener.incoming().take(1).compat();

            if let Some(socket) = strm.next().await {
                let tx = tx.clone();
                let session2 = session.clone();
                let mut session2 = session2.lock().await;
                let user = session2.user.clone();
                session2.process_data(user, socket.unwrap() /* TODO: Don't unwrap */, session.clone(), tx);
            }
        });

        Ok(Reply::new_with_string(
            ReplyCode::EnteringPassiveMode,
            format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
        ))
    }
}
