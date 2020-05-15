//! Contains code pertaining to the FTP *data* channel

use super::{
    chancomms::{DataCommand, InternalMsg},
    controlchan::command::Command,
    tls::FTPSConfig,
};
use crate::{
    auth::UserDetail,
    server::{tls::new_config, Session},
    storage::{Error, ErrorKind, Metadata, StorageBackend},
};
use futures::{channel::mpsc::Sender, prelude::*};
use log::{debug, error, info, warn};
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsAcceptor;

#[derive(Debug)]
pub struct DataCommandExecutor<S, U>
where
    S: StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
    U: UserDetail,
{
    pub user: Arc<Option<U>>,
    pub socket: tokio::net::TcpStream,
    pub control_msg_tx: Sender<InternalMsg>,
    pub storage: Arc<S>,
    pub cwd: PathBuf,
    pub start_pos: u64,
    pub ftps_mode: FTPSConfig,
}

impl<S, U: Send + Sync + 'static> DataCommandExecutor<S, U>
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
    U: UserDetail,
{
    #[tracing_attributes::instrument]
    pub async fn execute(self, cmd: Command) {
        match cmd {
            Command::Retr { path } => {
                self.exec_retr(path).await;
            }
            Command::Stor { path } => {
                self.exec_stor(path).await;
            }
            Command::List { path, .. } => {
                self.exec_list(path).await;
            }
            Command::Nlst { path } => {
                self.exec_nlst(path).await;
            }
            _ => unimplemented!(),
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_retr(self, path: String) {
        let path = self.cwd.join(path);
        let mut tx_sending: Sender<InternalMsg> = self.control_msg_tx.clone();
        let mut tx_error: Sender<InternalMsg> = self.control_msg_tx.clone();
        tokio::spawn(async move {
            match self.storage.get(&self.user, path, self.start_pos).await {
                Ok(mut f) => match tx_sending.send(InternalMsg::SendingData).await {
                    Ok(_) => {
                        let mut output = Self::writer(self.socket, self.ftps_mode);
                        match tokio::io::copy(&mut f, &mut output).await {
                            Ok(bytes_copied) => {
                                if let Err(err) = output.shutdown().await {
                                    warn!("Could not shutdown output stream after RETR: {}", err);
                                }
                                if let Err(err) = tx_sending.send(InternalMsg::SendData { bytes: bytes_copied as i64 }).await {
                                    error!("Could not notify control channel of successful RETR: {}", err);
                                }
                            }
                            Err(err) => warn!("Error copying streams during RETR: {}", err),
                        }
                    }
                    Err(err) => warn!("Error notifying control channel of progress during RETR: {}", err),
                },
                Err(err) => {
                    if let Err(err) = tx_error.send(InternalMsg::StorageError(err)).await {
                        warn!("Could not notify control channel of error with RETR: {}", err);
                    }
                }
            }
        });
    }

    #[tracing_attributes::instrument]
    async fn exec_stor(self, path: String) {
        let path = self.cwd.join(path);
        let mut tx_ok = self.control_msg_tx.clone();
        let mut tx_error = self.control_msg_tx.clone();
        tokio::spawn(async move {
            match self
                .storage
                .put(&self.user, Self::reader(self.socket, self.ftps_mode), path, self.start_pos)
                .await
            {
                Ok(bytes) => {
                    if let Err(err) = tx_ok.send(InternalMsg::WrittenData { bytes: bytes as i64 }).await {
                        error!("Could not notify control channel of successful STOR: {}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = tx_error.send(InternalMsg::StorageError(err)).await {
                        error!("Could not notify control channel of error with STOR: {}", err);
                    }
                }
            }
        });
    }

    #[tracing_attributes::instrument]
    async fn exec_list(self, path: Option<String>) {
        let path = match path {
            Some(path) => self.cwd.join(path),
            None => self.cwd.clone(),
        };
        let mut tx_ok = self.control_msg_tx.clone();
        tokio::spawn(async move {
            let mut output = Self::writer(self.socket, self.ftps_mode);
            let result = match self.storage.list_fmt(&self.user, path).await {
                Ok(cursor) => {
                    debug!("Copying future for List");
                    let mut input = cursor;
                    match tokio::io::copy(&mut input, &mut output).await {
                        Ok(_) => Ok(InternalMsg::DirectorySuccessfullyListed),
                        Err(e) => Err(e),
                    }
                }
                Err(err) => {
                    warn!("Failed to send directory list: {:?}", err);
                    match output.write_all(&format!("{}\r\n", err).into_bytes()).await {
                        Ok(_) => Ok(InternalMsg::DirectoryListFailure),
                        Err(e) => Err(e),
                    }
                }
            };
            match result {
                Ok(msg) => {
                    if let Err(err) = output.shutdown().await {
                        warn!("Could not shutdown output stream during LIST: {}", err);
                    }
                    if let Err(err) = tx_ok.send(msg).await {
                        error!("Could not notify control channel of LIST result: {}", err);
                    }
                }
                Err(err) => warn!("Failed to send reply to LIST: {}", err),
            }
        });
    }

    #[tracing_attributes::instrument]
    async fn exec_nlst(self, path: Option<String>) {
        let path = match path {
            Some(path) => self.cwd.join(path),
            None => self.cwd.clone(),
        };
        let mut tx_ok = self.control_msg_tx.clone();
        let mut tx_error = self.control_msg_tx.clone();
        tokio::spawn(async move {
            match self.storage.nlst(&self.user, path).await {
                Ok(mut input) => {
                    let mut output = Self::writer(self.socket, self.ftps_mode);
                    match tokio::io::copy(&mut input, &mut output).await {
                        Ok(_) => {
                            if let Err(err) = output.shutdown().await {
                                warn!("Could not shutdown output stream during NLIST: {}", err);
                            }
                            if let Err(err) = tx_ok.send(InternalMsg::DirectorySuccessfullyListed).await {
                                error!("Could not notify control channel of successful NLIST: {}", err);
                            }
                        }
                        Err(err) => warn!("Could not copy from storage implementation during NLST: {}", err),
                    }
                }
                Err(_) => {
                    if let Err(err) = tx_error.send(InternalMsg::StorageError(Error::from(ErrorKind::LocalError))).await {
                        warn!("Could not notify control channel of error with NLIST: {}", err);
                    }
                }
            }
        });
    }

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
    #[tracing_attributes::instrument]
    fn writer(socket: tokio::net::TcpStream, ftps_mode: FTPSConfig) -> Box<dyn tokio::io::AsyncWrite + Send + Unpin + Sync> {
        match ftps_mode {
            FTPSConfig::Off => Box::new(socket),
            FTPSConfig::On { certs_file, key_file } => {
                let io = futures::executor::block_on(async move {
                    let acceptor: TlsAcceptor = new_config(certs_file, key_file).into();
                    acceptor.accept(socket).await.unwrap()
                });
                Box::new(io)
            }
        }
    }

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
    #[tracing_attributes::instrument]
    fn reader(socket: tokio::net::TcpStream, ftps_mode: FTPSConfig) -> Box<dyn tokio::io::AsyncRead + Send + Unpin + Sync> {
        match ftps_mode {
            FTPSConfig::Off => Box::new(socket),
            FTPSConfig::On { certs_file, key_file } => {
                let io = futures::executor::block_on(async move {
                    let acceptor: TlsAcceptor = new_config(certs_file, key_file).into();
                    acceptor.accept(socket).await.unwrap()
                });
                Box::new(io)
            }
        }
    }
}

/// Processing for the data connection. This will spawn a new async task with the actual processing.
///
/// socket: the data socket we'll be working with
/// tls: tells if this should be a TLS connection
/// tx: channel to send the result of our operation to the control process
#[tracing_attributes::instrument]
pub fn spawn_processing<S, U>(session: &mut Session<S, U>, socket: tokio::net::TcpStream, tx: Sender<InternalMsg>)
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
    U: UserDetail + 'static,
{
    let mut data_cmd_rx = session.data_cmd_rx.take().unwrap().fuse();
    let mut data_abort_rx = session.data_abort_rx.take().unwrap().fuse();
    let ftps_mode = if session.data_tls { session.ftps_config.clone() } else { FTPSConfig::Off };
    let command_executor = DataCommandExecutor {
        user: session.user.clone(),
        socket,
        control_msg_tx: tx,
        storage: Arc::clone(&session.storage),
        cwd: session.cwd.clone(),
        start_pos: session.start_pos,
        ftps_mode,
    };

    tokio::spawn(async move {
        let mut timeout_delay = tokio::time::delay_for(std::time::Duration::from_secs(5 * 60));
        // TODO: Use configured timeout
        tokio::select! {
            Some(command) = data_cmd_rx.next() => {
                handle_incoming(DataCommand::ExternalCommand(command), command_executor).await;
            },
            Some(_) = data_abort_rx.next() => {
                handle_incoming(DataCommand::Abort, command_executor).await;
            },
            _ = &mut timeout_delay => {
                warn!("Data channel connection timed out");
                return;
            }
        };
    });
}

#[tracing_attributes::instrument]
async fn handle_incoming<S, U>(incoming: DataCommand, command_executor: DataCommandExecutor<S, U>)
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
    U: UserDetail + 'static,
{
    match incoming {
        DataCommand::Abort => {
            info!("Data channel abort received");
        }
        DataCommand::ExternalCommand(command) => {
            info!("Data command received: {:?}", command);
            command_executor.execute(command).await;
        }
    }
}
