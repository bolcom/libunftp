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

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use crate::{
    auth::UserDetail,
    server::{
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        chancomms::{DataChanCmd, ProxyLoopSender},
        session::SharedSession,
        ControlChanMsg, datachan
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug)]
pub struct Port {
    addr: String,
}

impl Port {
    pub fn new(addr: String) -> Self {
        Port { addr }
    }

    // modifies the session by adding channels that are used to communicate with the data connection
    // processing loop.
    #[tracing_attributes::instrument]
    async fn setup_inter_loop_comms<S, U>(&self, session: SharedSession<S, U>, control_loop_tx: Sender<ControlChanMsg>)
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        let (cmd_tx, cmd_rx): (Sender<DataChanCmd>, Receiver<DataChanCmd>) = channel(1);
        let (data_abort_tx, data_abort_rx): (Sender<()>, Receiver<()>) = channel(1);

        let mut session = session.lock().await;
        session.data_cmd_tx = Some(cmd_tx);
        session.data_cmd_rx = Some(cmd_rx);
        session.data_abort_tx = Some(data_abort_tx);
        session.data_abort_rx = Some(data_abort_rx);
        session.control_msg_tx = Some(control_loop_tx);
    }

    // For non-proxy mode we choose a data port here and start listening on it while letting the control
    // channel know (via method return) what the address is that the client should connect to.
    #[tracing_attributes::instrument]
    async fn handle_nonproxy_mode<S, U>(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        let CommandContext {
            logger,
            passive_host: _passive_host,
            tx_control_chan: tx,
            session,
            ..
        } = args;

        let bytes: Vec<u8> = self.addr.split(',').map(|x| x.parse::<u8>()).filter_map(Result::ok).collect();
        let port = ((bytes[4] as u16) << 8) | bytes[5] as u16;
        let addr = SocketAddrV4::new(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]), port);

        let stream: io::Result<TcpStream> = TcpStream::connect(addr).await;

        let stream = match stream {
            Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Ok(s) => s,
        };

        self.setup_inter_loop_comms(session.clone(), tx).await;
        datachan::spawn_processing(logger, session, stream).await;

        Ok(Reply::new(ReplyCode::CommandOkay, "Entering Active mode"))
    }

    #[tracing_attributes::instrument]
    async fn handle_proxy_mode<S, U>(&self, args: CommandContext<S, U>, tx: ProxyLoopSender<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        todo!()
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Port
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let sender: Option<ProxyLoopSender<Storage, User>> = args.tx_proxyloop.clone();
        match sender {
            Some(tx) => self.handle_proxy_mode(args, tx).await,
            None => self.handle_nonproxy_mode(args).await,
        }
    }
}
