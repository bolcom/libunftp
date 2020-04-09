//! Contains code pertaining to the FTP *data* channel

use super::chancomms::InternalMsg;
use super::commands::Command;
use super::storage::AsAsyncReads;
use crate::storage::{self, Error, ErrorKind};

use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub struct DataCommandExecutor<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    pub user: Arc<Option<U>>,
    pub socket: tokio::net::TcpStream,
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
        match cmd {
            Command::Retr { path } => {
                let path = self.cwd.join(path);
                let mut tx_sending: Sender<InternalMsg> = self.tx.clone();
                let mut tx_error: Sender<InternalMsg> = self.tx.clone();
                tokio::spawn(async move {
                    match self.storage.get(&self.user, path, self.start_pos).await {
                        Ok(f) => match tx_sending.send(InternalMsg::SendingData).await {
                            Ok(_) => {
                                let mut input = f.as_tokio02_async_read();
                                let mut output = Self::writer(self.socket, self.tls, self.identity_file, self.identity_password);
                                match tokio::io::copy(&mut input, &mut output).await {
                                    Ok(bytes_copied) => {
                                        if let Err(err) = output.shutdown().await {
                                            warn!("Could not shutdown output stream after RETR: {}", err);
                                        }
                                        if let Err(err) = tx_sending.send(InternalMsg::SendData { bytes: bytes_copied as i64 }).await {
                                            warn!("Could not notify control channel of successful RETR: {}", err);
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
            Command::Stor { path } => {
                let path = self.cwd.join(path);
                let mut tx_ok = self.tx.clone();
                let mut tx_error = self.tx.clone();
                tokio::spawn(async move {
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
                                warn!("Could not notify control channel of successful STOR: {}", err);
                            }
                        }
                        Err(err) => {
                            if let Err(err) = tx_error.send(InternalMsg::StorageError(err)).await {
                                warn!("Could not notify control channel of error with STOR: {}", err);
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
                tokio::spawn(async move {
                    match self.storage.list_fmt(&self.user, path).await {
                        Ok(cursor) => {
                            debug!("Copying future for List");
                            let mut input = cursor;
                            let mut output = Self::writer(self.socket, self.tls, self.identity_file, self.identity_password);
                            match tokio::io::copy(&mut input, &mut output).await {
                                Ok(_) => {
                                    if let Err(err) = output.shutdown().await {
                                        warn!("Could not shutdown output stream during LIST: {}", err);
                                    }
                                    if let Err(err) = tx_ok.send(InternalMsg::DirectorySuccessfullyListed).await {
                                        warn!("Could not notify control channel of successful LIST: {}", err);
                                    }
                                }
                                Err(err) => warn!("Could not copy from storage implementation during LIST: {}", err),
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
                tokio::spawn(async move {
                    match self.storage.nlst(&self.user, path).await {
                        Ok(mut input) => {
                            let mut output = Self::writer(self.socket, self.tls, self.identity_file, self.identity_password);
                            match tokio::io::copy(&mut input, &mut output).await {
                                Ok(_) => {
                                    if let Err(err) = output.shutdown().await {
                                        warn!("Could not shutdown output stream during NLIST: {}", err);
                                    }
                                    if let Err(err) = tx_ok.send(InternalMsg::DirectorySuccessfullyListed).await {
                                        warn!("Could not notify control channel of successful NLIST: {}", err);
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
            _ => unimplemented!(),
        }
    }

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
    fn writer(
        socket: tokio::net::TcpStream,
        tls: bool,
        identity_file: Option<PathBuf>,
        indentity_password: Option<String>,
    ) -> Box<dyn tokio::io::AsyncWrite + Send + Unpin + Sync> {
        if tls {
            let io = futures::executor::block_on(async move {
                let identity = crate::server::tls::identity(identity_file.unwrap(), indentity_password.unwrap());
                let acceptor = tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                acceptor.accept(socket).await.unwrap()
            });
            Box::new(io)
        } else {
            Box::new(socket)
        }
    }

    // Lots of code duplication here. Should disappear completely when the storage backends are rewritten in async/.await style
    fn reader(
        socket: tokio::net::TcpStream,
        tls: bool,
        identity_file: Option<PathBuf>,
        indentity_password: Option<String>,
    ) -> Box<dyn tokio::io::AsyncRead + Send + Unpin + Sync> {
        if tls {
            let io = futures::executor::block_on(async move {
                let identity = crate::server::tls::identity(identity_file.unwrap(), indentity_password.unwrap());
                let acceptor = tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                acceptor.accept(socket).await.unwrap()
            });
            Box::new(io)
        } else {
            Box::new(socket)
        }
    }
}
