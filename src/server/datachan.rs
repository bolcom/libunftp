//! Contains code pertaining to the FTP *data* channel

use super::chancomms::InternalMsg;
use super::commands::Command;
use super::storage::AsAsyncReads;
use crate::storage::{self, Error, ErrorKind};

use futures03::channel::mpsc::Sender;
use futures03::compat::*;
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;

pub struct DataCommandExecutor<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    pub user: Arc<Option<U>>,
    pub socket: tokio02::net::TcpStream,
    pub tls: bool,
    pub tx: Sender<InternalMsg>,
    pub storage: Arc<S>,
    pub cwd: PathBuf,
    pub start_pos: u64,
    pub identity_file: Option<PathBuf>,
    pub identity_password: Option<String>,
}

impl<S, U: Send + Sync + 'static> DataCommandExecutor<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    pub async fn execute(self, cmd: Command) {
        use futures03::prelude::*;
        match cmd {
            Command::Retr { path } => {
                let path = self.cwd.join(path);
                let mut tx_sending: Sender<InternalMsg> = self.tx.clone();
                let mut tx_error: Sender<InternalMsg> = self.tx.clone();
                tokio02::spawn(async move {
                    match self.storage.get(&self.user, path, self.start_pos).await {
                        Ok(f) => match tx_sending.send(InternalMsg::SendingData).await {
                            Ok(_) => match tokio::io::copy(
                                f.as_tokio01_async_read(),
                                Self::writer(self.socket, self.tls, self.identity_file, self.identity_password),
                            )
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
            Command::Stor { path } => {
                let path = self.cwd.join(path);
                let mut tx_ok = self.tx.clone();
                let mut tx_error = self.tx.clone();
                tokio02::spawn(async move {
                    match self
                        .storage
                        .put(
                            &self.user,
                            Self::reader(self.socket, self.tls, self.identity_file, self.identity_password),
                            path,
                            self.start_pos,
                        )
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
            Command::List { path, .. } => {
                let path = match path {
                    Some(path) => self.cwd.join(path),
                    None => self.cwd.clone(),
                };
                let mut tx_ok = self.tx.clone();
                tokio02::spawn(async move {
                    match self.storage.list_fmt(&self.user, path).await {
                        Ok(cursor) => {
                            debug!("Copying future for List");
                            match tokio::io::copy(cursor, Self::writer(self.socket, self.tls, self.identity_file, self.identity_password))
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
            Command::Nlst { path } => {
                let path = match path {
                    Some(path) => self.cwd.join(path),
                    None => self.cwd.clone(),
                };
                let mut tx_ok = self.tx.clone();
                let mut tx_error = self.tx.clone();
                tokio02::spawn(async move {
                    match self.storage.nlst(&self.user, path).await {
                        Ok(res) => match tokio::io::copy(res, Self::writer(self.socket, self.tls, self.identity_file, self.identity_password))
                            .compat()
                            .await
                        {
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
            _ => unimplemented!(),
        }
    }

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
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

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
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
}
