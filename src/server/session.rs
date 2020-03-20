//! The session module implements per-connection session handling and currently also
//! implements the control loop for the *data* channel.

use super::chancomms::{DataCommand, InternalMsg};
use super::commands::Command;
use super::storage::AsAsyncReads;
use crate::metrics;
use crate::storage::{self, Error, ErrorKind};
use futures::prelude::*;

use futures03::channel::mpsc::Receiver;
use futures03::channel::mpsc::Sender;
use futures03::compat::*;
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(PartialEq)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// This is where we keep the state for a ftp session.
pub struct Session<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    pub user: Arc<Option<U>>,
    pub username: Option<String>,
    pub storage: Arc<S>,
    pub data_cmd_tx: Option<futures03::channel::mpsc::Sender<Command>>,
    pub data_cmd_rx: Option<Receiver<Command>>,
    pub data_abort_tx: Option<futures03::channel::mpsc::Sender<()>>,
    pub data_abort_rx: Option<Receiver<()>>,
    pub cwd: std::path::PathBuf,
    pub rename_from: Option<PathBuf>,
    pub state: SessionState,
    pub certs_file: Option<PathBuf>,
    pub certs_password: Option<String>,
    // True if the command channel is in secure mode
    pub cmd_tls: bool,
    // True if the data channel is in secure mode.
    pub data_tls: bool,
    pub with_metrics: bool,
    // The starting byte for a STOR or RETR command. Set by the _Restart of Interrupted Transfer (REST)_
    // command to support resume functionality.
    pub start_pos: u64,
}

impl<S, U: Send + Sync + 'static> Session<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    pub(super) fn with_storage(storage: Arc<S>) -> Self {
        Session {
            user: Arc::new(None),
            username: None,
            storage,
            data_cmd_tx: None,
            data_cmd_rx: None,
            data_abort_tx: None,
            data_abort_rx: None,
            cwd: "/".into(),
            rename_from: None,
            state: SessionState::New,
            certs_file: Option::None,
            certs_password: Option::None,
            cmd_tls: false,
            data_tls: false,
            with_metrics: false,
            start_pos: 0,
        }
    }

    pub(super) fn with_ftps(mut self, certs_file: Option<PathBuf>, password: Option<String>) -> Self {
        self.certs_file = certs_file;
        self.certs_password = password;
        self
    }

    pub(super) fn with_metrics(mut self, with_metrics: bool) -> Self {
        if with_metrics {
            metrics::inc_session();
        }
        self.with_metrics = with_metrics;
        self
    }

    // Lots of code duplication here. Should disappear completely when this module in rewritten in async/.await style
    fn writer(
        socket: tokio02::net::TcpStream,
        tls: bool,
        identity_file: Option<PathBuf>,
        indentity_password: Option<String>,
    ) -> Box<dyn tokio::io::AsyncWrite + Send> {
        use futures03::AsyncReadExt;
        use tokio02util::compat::Tokio02AsyncReadCompatExt;
        if tls {
            let io = futures03::executor::block_on(async move {
                let identity = crate::server::tls::identity(identity_file.unwrap(), indentity_password.unwrap());
                let acceptor = tokio02tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                acceptor.accept(socket).await.unwrap()
            });
            let futures03_async_read = io.compat();
            Box::new(futures03_async_read.compat())
        } else {
            let futures03_async_read = socket.compat();
            Box::new(futures03_async_read.compat())
        }
    }

    // Lots of code duplication here. Should disappear completely when this module in rewritten in async/.await style
    fn reader(
        socket: tokio02::net::TcpStream,
        tls: bool,
        identity_file: Option<PathBuf>,
        indentity_password: Option<String>,
    ) -> Box<dyn tokio::io::AsyncRead + Send> {
        use futures03::AsyncReadExt;
        use tokio02util::compat::Tokio02AsyncReadCompatExt;
        if tls {
            let io = futures03::executor::block_on(async move {
                let identity = crate::server::tls::identity(identity_file.unwrap(), indentity_password.unwrap());
                let acceptor = tokio02tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                acceptor.accept(socket).await.unwrap()
            });
            let futures03_async_read = io.compat();
            Box::new(futures03_async_read.compat())
        } else {
            let futures03_async_read = socket.compat();
            Box::new(futures03_async_read.compat())
        }
    }

    /// Processing for the data connection.
    ///
    /// socket: the data socket we'll be working with
    /// tls: tells if this should be a TLS connection
    /// tx: channel to send the result of our operation to the control process
    pub(super) fn process_data(&mut self, user: Arc<Option<U>>, socket: tokio02::net::TcpStream, tls: bool, tx: Sender<InternalMsg>) {
        // TODO: Either take the rx as argument, or properly check the result instead of
        // `unwrap()`.
        let rx = {
            use futures03::stream::StreamExt;
            use futures03::stream::TryStreamExt;
            self.data_cmd_rx.take().unwrap().map(Ok::<Command, ()>).compat()
        };
        // TODO: Same as above, don't `unwrap()` here. Ideally we solve this by refactoring to a
        // proper state machine.
        let abort_rx: Receiver<()> = self.data_abort_rx.take().unwrap();
        let storage: Arc<S> = Arc::clone(&self.storage);
        let cwd = self.cwd.clone();
        let start_pos: u64 = self.start_pos;
        let identity_file = if tls { Some(self.certs_file.clone().unwrap()) } else { None };
        let identity_password = if tls { Some(self.certs_password.clone().unwrap()) } else { None };
        let task = rx
            .take(1)
            .map(DataCommand::ExternalCommand)
            .select({
                use futures03::stream::StreamExt;
                use futures03::stream::TryStreamExt;
                abort_rx.map(|_| Ok(DataCommand::Abort)).compat()
            })
            .take_while(|data_cmd| Ok(*data_cmd != DataCommand::Abort))
            .into_future()
            .map(move |(cmd, _)| {
                use self::DataCommand::ExternalCommand;
                use futures03::prelude::*;
                match cmd {
                    Some(ExternalCommand(Command::Retr { path })) => {
                        let path = cwd.join(path);
                        let mut tx_sending: Sender<InternalMsg> = tx.clone();
                        let mut tx_error: Sender<InternalMsg> = tx.clone();
                        tokio02::spawn(async move {
                            match storage.get(&user, path, start_pos).compat().await {
                                Ok(f) => match tx_sending.send(InternalMsg::SendingData).await {
                                    Ok(_) => match tokio::io::copy(f.as_tokio01_async_read(), Self::writer(socket, tls, identity_file, identity_password))
                                        .compat()
                                        .await
                                    {
                                        Ok((bytes, _, _)) => {
                                            if let Err(err) = tx_sending.send(InternalMsg::SendData { bytes: bytes as i64 }).await {
                                                warn!("{}", err);
                                            }
                                        }
                                        Err(err) => warn!("{}", err),
                                    },
                                    Err(err) => warn!("{}", err),
                                },
                                Err(err) => {
                                    if let Err(err) = tx_error.send(InternalMsg::StorageError(err)).await {
                                        warn!("{}", err);
                                    }
                                }
                            }
                        });
                    }
                    Some(ExternalCommand(Command::Stor { path })) => {
                        let path = cwd.join(path);
                        let mut tx_ok = tx.clone();
                        let mut tx_error = tx.clone();
                        tokio02::spawn(async move {
                            match storage
                                .put(&user, Self::reader(socket, tls, identity_file, identity_password), path, start_pos)
                                .compat()
                                .await
                            {
                                Ok(bytes) => {
                                    if let Err(err) = tx_ok.send(InternalMsg::WrittenData { bytes: bytes as i64 }).await {
                                        warn!("{}", err);
                                    }
                                }
                                Err(err) => {
                                    if let Err(err) = tx_error.send(InternalMsg::StorageError(err)).await {
                                        warn!("{}", err);
                                    }
                                }
                            }
                        });
                    }
                    Some(ExternalCommand(Command::List { path, .. })) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let mut tx_ok = tx.clone();
                        tokio02::spawn(async move {
                            match storage.list_fmt(&user, path).compat().await {
                                Ok(cursor) => {
                                    debug!("Copying future for List");
                                    match tokio::io::copy(cursor, Self::writer(socket, tls, identity_file, identity_password))
                                        .compat()
                                        .await
                                    {
                                        Ok(reader_writer) => {
                                            debug!("Shutdown future for List");
                                            let tcp_tls_stream = reader_writer.2;
                                            match tokio::io::shutdown(tcp_tls_stream).compat().await {
                                                Ok(_) => {
                                                    if let Err(err) = tx_ok.send(InternalMsg::DirectorySuccessfullyListed).await {
                                                        warn!("{}", err);
                                                    }
                                                }
                                                Err(err) => warn!("{}", err),
                                            }
                                        }
                                        Err(err) => warn!("{}", err),
                                    }
                                }
                                Err(err) => warn!("Failed to send directory list: {:?}", err),
                            }
                        });
                    }
                    Some(ExternalCommand(Command::Nlst { path })) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let mut tx_ok = tx.clone();
                        let mut tx_error = tx.clone();
                        tokio02::spawn(async move {
                            match storage.nlst(&user, path).compat().await {
                                Ok(res) => match tokio::io::copy(res, Self::writer(socket, tls, identity_file, identity_password)).compat().await {
                                    Ok(_) => {
                                        if let Err(err) = tx_ok.send(InternalMsg::DirectorySuccessfullyListed).await {
                                            warn!("{}", err);
                                        }
                                    }
                                    Err(err) => warn!("{}", err),
                                },
                                Err(_) => {
                                    if let Err(err) = tx_error.send(InternalMsg::StorageError(Error::from(ErrorKind::LocalError))).await {
                                        warn!("{}", err)
                                    }
                                }
                            }
                        });
                    }
                    // TODO: Remove catch-all Some(_) when I'm done implementing :)
                    Some(ExternalCommand(_)) => unimplemented!(),
                    Some(DataCommand::Abort) => unreachable!(),
                    None => { /* This probably happened because the control channel was closed before we got here */ }
                }
            })
            .into_future()
            .map_err(|_| ())
            .map(|_| ());

        tokio::spawn(task);
    }
}

impl<S, U: Send + Sync> Drop for Session<S, U>
where
    S: storage::StorageBackend<U>,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    fn drop(&mut self) {
        if self.with_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
