//! Contains code pertaining to the FTP *data* channel

use super::{
    chancomms::{ControlChanMsg, DataChanMsg},
    tls::FTPSConfig,
};
use crate::server::session::SharedSession;
use crate::{
    auth::UserDetail,
    server::tls::new_config,
    storage::{Error, ErrorKind, Metadata, StorageBackend},
};

use crate::server::chancomms::DataChanCmd;
use futures::{
    channel::mpsc::{Receiver, Sender},
    prelude::*,
};
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsAcceptor;

#[derive(Debug)]
struct DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User>,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    pub user: Arc<Option<User>>,
    pub socket: tokio::net::TcpStream,
    pub control_msg_tx: Sender<ControlChanMsg>,
    pub storage: Arc<Storage>,
    pub cwd: PathBuf,
    pub start_pos: u64,
    pub ftps_mode: FTPSConfig,
    pub logger: slog::Logger,
    pub data_cmd_rx: Option<Receiver<DataChanCmd>>,
    pub data_abort_rx: Option<Receiver<()>>,
}

impl<Storage, User> DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn execute(mut self, session_arc: SharedSession<Storage, User>) {
        let mut data_cmd_rx = self.data_cmd_rx.take().unwrap().fuse();
        let mut data_abort_rx = self.data_abort_rx.take().unwrap().fuse();
        let mut timeout_delay = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(5 * 60)));
        // TODO: Use configured timeout
        tokio::select! {
            Some(command) = data_cmd_rx.next() => {
                self.handle_incoming(DataChanMsg::ExternalCommand(command)).await;
            },
            Some(_) = data_abort_rx.next() => {
                self.handle_incoming(DataChanMsg::Abort).await;
            },
            _ = &mut timeout_delay => {
                slog::warn!(self.logger, "Data channel connection timed out");
            }
        };
        let mut session = session_arc.lock().await;
        session.data_busy = false;
    }

    #[tracing_attributes::instrument]
    async fn handle_incoming(self, incoming: DataChanMsg) {
        match incoming {
            DataChanMsg::Abort => {
                slog::info!(self.logger, "Data channel abort received");
            }
            DataChanMsg::ExternalCommand(command) => {
                slog::info!(self.logger, "Data channel command received: {:?}", command);
                self.execute_command(command).await;
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn execute_command(self, cmd: DataChanCmd) {
        match cmd {
            DataChanCmd::Retr { path } => {
                self.exec_retr(path).await;
            }
            DataChanCmd::Stor { path } => {
                self.exec_stor(path).await;
            }
            DataChanCmd::List { path, .. } => {
                self.exec_list(path).await;
            }
            DataChanCmd::Nlst { path } => {
                self.exec_nlst(path).await;
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_retr(self, path: String) {
        let path = self.cwd.join(path);
        let mut tx_sending: Sender<ControlChanMsg> = self.control_msg_tx.clone();
        let mut tx_error: Sender<ControlChanMsg> = self.control_msg_tx.clone();
        let mut output = Self::writer(self.socket, self.ftps_mode).await;
        let get_result = self.storage.get_into(&self.user, path, self.start_pos, &mut output).await;
        match get_result {
            Ok(bytes_copied) => {
                if let Err(err) = output.shutdown().await {
                    slog::warn!(self.logger, "Could not shutdown output stream after RETR: {}", err);
                }
                if let Err(err) = tx_sending.send(ControlChanMsg::SendData { bytes: bytes_copied as i64 }).await {
                    slog::error!(self.logger, "Could not notify control channel of successful RETR: {}", err);
                }
            }
            Err(err) => {
                slog::warn!(self.logger, "Error copying streams during RETR: {}", err);
                if let Err(err) = tx_error.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(self.logger, "Could not notify control channel of error with RETR: {}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_stor(self, path: String) {
        let path = self.cwd.join(path);
        let mut tx_ok = self.control_msg_tx.clone();
        let mut tx_error = self.control_msg_tx.clone();
        let put_result = self
            .storage
            .put(&self.user, Self::reader(self.socket, self.ftps_mode).await, path, self.start_pos)
            .await;
        match put_result {
            Ok(bytes) => {
                if let Err(err) = tx_ok.send(ControlChanMsg::WrittenData { bytes: bytes as i64 }).await {
                    slog::error!(self.logger, "Could not notify control channel of successful STOR: {}", err);
                }
            }
            Err(err) => {
                if let Err(err) = tx_error.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with STOR: {}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_list(self, path: Option<String>) {
        let path = match path {
            Some(path) => {
                if path == "." {
                    self.cwd.clone()
                } else {
                    self.cwd.join(path)
                }
            }
            None => self.cwd.clone(),
        };
        let mut tx_ok = self.control_msg_tx.clone();
        let mut output = Self::writer(self.socket, self.ftps_mode).await;
        let result = match self.storage.list_fmt(&self.user, path).await {
            Ok(cursor) => {
                slog::debug!(self.logger, "Copying future for List");
                let mut input = cursor;
                match tokio::io::copy(&mut input, &mut output).await {
                    Ok(_) => Ok(ControlChanMsg::DirectorySuccessfullyListed),
                    Err(e) => Err(e),
                }
            }
            Err(err) => {
                slog::warn!(self.logger, "Failed to send directory list: {:?}", err);
                match output.write_all(&format!("{}\r\n", err).into_bytes()).await {
                    Ok(_) => Ok(ControlChanMsg::DirectoryListFailure),
                    Err(e) => Err(e),
                }
            }
        };
        match result {
            Ok(msg) => {
                if let Err(err) = output.shutdown().await {
                    slog::warn!(self.logger, "Could not shutdown output stream during LIST: {}", err);
                }
                if let Err(err) = tx_ok.send(msg).await {
                    slog::error!(self.logger, "Could not notify control channel of LIST result: {}", err);
                }
            }
            Err(err) => slog::warn!(self.logger, "Failed to send reply to LIST: {}", err),
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_nlst(self, path: Option<String>) {
        let path = match path {
            Some(path) => self.cwd.join(path),
            None => self.cwd.clone(),
        };
        let mut tx_ok = self.control_msg_tx.clone();
        let mut tx_error = self.control_msg_tx.clone();
        match self.storage.nlst(&self.user, path).await {
            Ok(mut input) => {
                let mut output = Self::writer(self.socket, self.ftps_mode).await;
                match tokio::io::copy(&mut input, &mut output).await {
                    Ok(_) => {
                        if let Err(err) = output.shutdown().await {
                            slog::warn!(self.logger, "Could not shutdown output stream during NLIST: {}", err);
                        }
                        if let Err(err) = tx_ok.send(ControlChanMsg::DirectorySuccessfullyListed).await {
                            slog::error!(self.logger, "Could not notify control channel of successful NLIST: {}", err);
                        }
                    }
                    Err(err) => slog::warn!(self.logger, "Could not copy from storage implementation during NLST: {}", err),
                }
            }
            Err(e) => {
                if let Err(err) = tx_error.send(ControlChanMsg::StorageError(Error::new(ErrorKind::LocalError, e))).await {
                    slog::warn!(self.logger, "Could not notify control channel of error with NLIST: {}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn writer(socket: tokio::net::TcpStream, ftps_mode: FTPSConfig) -> Box<dyn tokio::io::AsyncWrite + Send + Unpin + Sync> {
        match ftps_mode {
            FTPSConfig::Off => Box::new(socket) as Box<dyn tokio::io::AsyncWrite + Send + Unpin + Sync>,
            FTPSConfig::On { certs_file, key_file } => {
                let io = async move {
                    let acceptor: TlsAcceptor = new_config(certs_file, key_file).into();
                    acceptor.accept(socket).await.unwrap()
                }
                .await;
                Box::new(io) as Box<dyn tokio::io::AsyncWrite + Send + Unpin + Sync>
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn reader(socket: tokio::net::TcpStream, ftps_mode: FTPSConfig) -> Box<dyn tokio::io::AsyncRead + Send + Unpin + Sync> {
        match ftps_mode {
            FTPSConfig::Off => Box::new(socket) as Box<dyn tokio::io::AsyncRead + Send + Unpin + Sync>,
            FTPSConfig::On { certs_file, key_file } => {
                let io = async move {
                    let acceptor: TlsAcceptor = new_config(certs_file, key_file).into();
                    acceptor.accept(socket).await.unwrap()
                }
                .await;
                Box::new(io) as Box<dyn tokio::io::AsyncRead + Send + Unpin + Sync>
            }
        }
    }
}

/// Starts processing for the data connection. This will spawn a new async task that will wait for
/// a command from the control channel after which it will start to process the specified socket
/// that is connected to the client.
///
/// logger: logger set up with needed context for use by the data channel.
/// session_arc: the user session that is also shared with the control channel.
/// socket: the data socket we'll be working with.
#[tracing_attributes::instrument]
pub async fn spawn_processing<Storage, User>(logger: slog::Logger, session_arc: SharedSession<Storage, User>, mut socket: tokio::net::TcpStream)
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    // We introduce a block scope here to keep the lock on the session minimal. We basically copy the needed info
    // out and then unlock.

    let command_executor = {
        let mut session = session_arc.lock().await;

        match socket.peer_addr() {
            Ok(datachan_addr) => {
                let controlcahn_ip = session.source.ip();
                if controlcahn_ip != datachan_addr.ip() {
                    if let Err(err) = socket.shutdown().await {
                        slog::error!(
                            logger,
                            "Couldn't close datachannel for ip ({}) that does not match the ip({}) of the control channel.\n{:?}",
                            datachan_addr.ip(),
                            controlcahn_ip,
                            err
                        )
                    } else {
                        slog::warn!(
                            logger,
                            "Closing datachannel for ip ({}) that does not match the ip({}) of the control channel.",
                            datachan_addr.ip(),
                            controlcahn_ip
                        )
                    }
                    return;
                }
            }
            Err(err) => {
                slog::error!(logger, "Couldn't determine data channel address.\n{:?}", err);
                return;
            }
        }

        let username = session.username.as_ref().cloned().unwrap_or_else(|| String::from("unknown"));
        let logger = logger.new(slog::o!("username" => username));
        let control_msg_tx: Sender<ControlChanMsg> = match session.control_msg_tx {
            Some(ref tx) => tx.clone(),
            None => {
                slog::error!(logger, "Control loop message sender expected to be set up. Aborting data loop.");
                return;
            }
        };
        let data_cmd_rx = match session.data_cmd_rx.take() {
            Some(rx) => rx,
            None => {
                slog::error!(logger, "Data loop command receiver expected to be set up. Aborting data loop.");
                return;
            }
        };
        let data_abort_rx = match session.data_abort_rx.take() {
            Some(rx) => rx,
            None => {
                slog::error!(logger, "Data loop abort receiver expected to be set up. Aborting data loop.");
                return;
            }
        };
        let ftps_mode = if session.data_tls { session.ftps_config.clone() } else { FTPSConfig::Off };
        let command_executor = DataCommandExecutor {
            user: session.user.clone(),
            socket,
            control_msg_tx,
            storage: Arc::clone(&session.storage),
            cwd: session.cwd.clone(),
            start_pos: session.start_pos,
            ftps_mode,
            logger,
            data_abort_rx: Some(data_abort_rx),
            data_cmd_rx: Some(data_cmd_rx),
        };

        // The control channel need to know if the data channel is busy so that it doesn't time out
        // while the session is still in progress.
        session.data_busy = true;

        command_executor
    };

    tokio::spawn(command_executor.execute(session_arc));
}
