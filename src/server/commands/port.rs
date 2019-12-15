//! The RFC 959 Data Port (`PORT`) command
//
// The argument is a HOST-PORT specification for the data port
// to be used in data connection.  There are defaults for both
// the user and server data ports, and under normal
// circumstances this command and its reply are not needed.  If
// this command is used, the argument is the concatenation of a
// 32-bit internet host address and a 16-bit TCP port address.
// This address information is broken into 8-bit fields and the
// value of each field is transmitted as a decimal number (in
// character string representation).  The fields are separated
// by commas.  A port command would be:
//
// PORT h1,h2,h3,h4,p1,p2
//
// where h1 is the high order 8 bits of the internet host
// address.

use crate::server::commands::{Cmd, Command};
use crate::server::error::{FTPError, FTPErrorKind};
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use futures::sync::mpsc;
use futures::Future;
use log::{error, trace, warn};
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::net::TcpStream;

//pub struct Port;
pub struct Port {
    addr: String,
}

impl Port {
    pub fn new(addr: String) -> Self {
        Port { addr }
    }
}

impl<S, U> Cmd<S, U> for Port
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let bytes: Vec<u8> = self.addr.split(',').map(|x| x.parse::<u8>()).filter_map(Result::ok).collect();
        if bytes.len() != 6 {
            return Err(FTPErrorKind::ParseError.into());
        }
        let port = ((bytes[4] as u16) << 8) | bytes[5] as u16;
        let addr = SocketAddrV4::new(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]), port);

        let tx = args.tx.clone();
        let (cmd_tx, cmd_rx): (mpsc::Sender<Command>, mpsc::Receiver<Command>) = mpsc::channel(1);
        let (data_abort_tx, data_abort_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);
        {
            let mut session = args.session.lock()?;
            session.data_cmd_tx = Some(cmd_tx);
            session.data_cmd_rx = Some(cmd_rx);
            session.data_abort_tx = Some(data_abort_tx);
            session.data_abort_rx = Some(data_abort_rx);
        }

        let stream = TcpStream::connect(&addr.into());
        let session = args.session.clone();
        let client = stream
            .map(move |socket| {
                trace!("Active socket Connected {}", addr.clone());
                let tx = tx.clone();
                let session2 = session.clone();
                let mut session2 = session2.lock().unwrap_or_else(|res| {
                    // TODO: Send signal to `tx` here, so we can handle the
                    // error
                    error!("session lock() result: {}", res);
                    panic!()
                });
                let user = session2.user.clone();
                session2.process_data(user, socket, session.clone(), tx);
            })
            .map_err(|e| warn!("Failed to connect data socket: {:?}", e));
        tokio::spawn(Box::new(client));

        Ok(Reply::new(ReplyCode::CommandOkay, "Entering Active Mode"))
    }
}
