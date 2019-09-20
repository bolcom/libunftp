use std::io::ErrorKind;
use std::sync::{Arc, Mutex};

use futures::prelude::*;
use futures::sync::mpsc;
use futures::Sink;
use log::warn;
use tokio::net::TcpStream;

use super::chancomms::{DataCommand, InternalMsg};
use super::commands::Command;
use super::stream::{SecurityState, SecuritySwitch, SwitchingTlsStream};
use crate::storage::{self, ErrorSemantics};

const DATA_CHANNEL_ID: u8 = 1;

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
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    pub user: Arc<Option<U>>,
    pub username: Option<String>,
    pub storage: Arc<S>,
    pub data_cmd_tx: Option<mpsc::Sender<Command>>,
    pub data_cmd_rx: Option<mpsc::Receiver<Command>>,
    pub data_abort_tx: Option<mpsc::Sender<()>>,
    pub data_abort_rx: Option<mpsc::Receiver<()>>,
    pub cwd: std::path::PathBuf,
    pub rename_from: Option<std::path::PathBuf>,
    pub state: SessionState,
    pub certs_file: Option<&'static str>,
    pub key_file: Option<&'static str>,
    // True if the command channel is in secure mode
    pub cmd_tls: bool,
    // True if the data channel is in secure mode.
    pub data_tls: bool,
}

impl<S, U: Send + Sync + 'static> Session<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
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
            key_file: Option::None,
            cmd_tls: false,
            data_tls: false,
        }
    }

    pub(super) fn certs(mut self, certs_file: Option<&'static str>, key_file: Option<&'static str>) -> Self {
        self.certs_file = certs_file;
        self.key_file = key_file;
        self
    }

    /// Processing for the data connection.
    ///
    /// socket: the data socket we'll be working with
    /// sec_switch: communicates the security setting for the data channel.
    /// tx: channel to send the result of our operation to the control process
    pub(super) fn process_data(&mut self, user: Arc<Option<U>>, socket: TcpStream, sec_switch: Arc<Mutex<Session<S, U>>>, tx: mpsc::Sender<InternalMsg>) {
        let tcp_tls_stream: Box<dyn crate::server::AsyncStream> = match (self.certs_file, self.key_file) {
            (Some(certs), Some(keys)) => Box::new(SwitchingTlsStream::new(socket, sec_switch, DATA_CHANNEL_ID, certs, keys)),
            _ => Box::new(socket),
        };

        // TODO: Either take the rx as argument, or properly check the result instead of
        // `unwrap()`.
        let rx = self.data_cmd_rx.take().unwrap();
        // TODO: Same as above, don't `unwrap()` here. Ideally we solve this by refactoring to a
        // proper state machine.
        let abort_rx = self.data_abort_rx.take().unwrap();
        let storage = Arc::clone(&self.storage);
        let cwd = self.cwd.clone();
        let task = rx
            .take(1)
            .map(DataCommand::ExternalCommand)
            .select(abort_rx.map(|_| DataCommand::Abort))
            .take_while(|data_cmd| Ok(*data_cmd != DataCommand::Abort))
            .into_future()
            .map(move |(cmd, _)| {
                use self::DataCommand::ExternalCommand;
                match cmd {
                    Some(ExternalCommand(Command::Retr { path })) => {
                        let path = cwd.join(path);
                        let tx_sending = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage
                                .get(&user, path)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to get file"))
                                .and_then(|f| {
                                    tx_sending
                                        .send(InternalMsg::SendingData)
                                        .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'SendingData' message to data channel"))
                                        .and_then(|_| tokio_io::io::copy(f, tcp_tls_stream))
                                        .and_then(|(bytes, _, _)| {
                                            tx.send(InternalMsg::SendData { bytes: bytes as i64 })
                                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'SendData' message to data channel"))
                                        })
                                })
                                .or_else(|e| {
                                    let msg = match e.kind() {
                                        ErrorKind::NotFound => InternalMsg::NotFound,
                                        ErrorKind::PermissionDenied => InternalMsg::PermissionDenied,
                                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => InternalMsg::ConnectionReset,
                                        _ => InternalMsg::UnknownRetrieveError,
                                    };
                                    tx_error
                                        .send(msg)
                                        .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send ErrorMessage to data channel"))
                                })
                                .map(|_| ())
                                .map_err(|e| {
                                    warn!("Failed to send file: {:?}", e);
                                }),
                        );
                    }
                    Some(ExternalCommand(Command::Stor { path })) => {
                        let path = cwd.join(path);
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage
                                .put(&user, tcp_tls_stream, path)
                                .map_err(|e| {
                                    if let Some(kind) = e.io_error_kind() {
                                        std::io::Error::new(kind, "Failed to put file")
                                    } else {
                                        std::io::Error::new(std::io::ErrorKind::Other, "Failed to put file")
                                    }
                                })
                                .and_then(|bytes| {
                                    tx_ok
                                        .send(InternalMsg::WrittenData { bytes: bytes as i64 })
                                        .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send WrittenData to the control channel"))
                                })
                                .or_else(|e| {
                                    let msg = match e.kind() {
                                        ErrorKind::NotFound => InternalMsg::NotFound,
                                        ErrorKind::PermissionDenied => InternalMsg::PermissionDenied,
                                        ErrorKind::ConnectionReset => InternalMsg::ConnectionReset,
                                        ErrorKind::ConnectionAborted => InternalMsg::DataConnectionClosedAfterStor,
                                        _ => InternalMsg::WriteFailed,
                                    };
                                    tx_error.send(msg)
                                })
                                .map(|_| ())
                                .map_err(|e| {
                                    warn!("Failed to send file: {:?}", e);
                                }),
                        );
                    }
                    Some(ExternalCommand(Command::List { path, .. })) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage
                                .list_fmt(&user, path)
                                .and_then(|res| tokio::io::copy(res, tcp_tls_stream))
                                .and_then(|_| {
                                    tx_ok
                                        .send(InternalMsg::DirectorySuccessfullyListed)
                                        .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to Send `DirectorySuccesfullyListed` event"))
                                })
                                .or_else(|e| {
                                    let msg = match e.kind() {
                                        // TODO: Consider making these events unique (so don't reuse
                                        // the `Stor` messages here)
                                        ErrorKind::NotFound => InternalMsg::NotFound,
                                        ErrorKind::PermissionDenied => InternalMsg::PermissionDenied,
                                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => InternalMsg::ConnectionReset,
                                        _ => InternalMsg::WriteFailed,
                                    };
                                    tx_error.send(msg)
                                })
                                .map(|_| ())
                                .map_err(|e| {
                                    warn!("Failed to send directory list: {:?}", e);
                                }),
                        );
                    }
                    Some(ExternalCommand(Command::Nlst { path })) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage
                                .nlst(&user, path)
                                .and_then(|res| tokio::io::copy(res, tcp_tls_stream))
                                .and_then(|_| {
                                    tx_ok
                                        .send(InternalMsg::DirectorySuccessfullyListed)
                                        .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to Send `DirectorySuccesfullyListed` event"))
                                })
                                .or_else(|e| {
                                    let msg = match e.kind() {
                                        // TODO: Consider making these events unique (so don't reuse
                                        // the `Stor` messages here)
                                        ErrorKind::NotFound => InternalMsg::NotFound,
                                        ErrorKind::PermissionDenied => InternalMsg::PermissionDenied,
                                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => InternalMsg::ConnectionReset,
                                        _ => InternalMsg::WriteFailed,
                                    };
                                    tx_error.send(msg)
                                })
                                .map(|_| ())
                                .map_err(|e| {
                                    warn!("Failed to send directory list: {:?}", e);
                                }),
                        );
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

impl<S, U: Send + Sync + 'static> SecuritySwitch for Session<S, U>
where
    S: storage::StorageBackend<U>,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    fn which_state(&self, channel: u8) -> SecurityState {
        match channel {
            crate::server::CONTROL_CHANNEL_ID => {
                if self.cmd_tls {
                    return SecurityState::On;
                }
                SecurityState::Off
            }
            DATA_CHANNEL_ID => {
                if self.data_tls {
                    return SecurityState::On;
                }
                SecurityState::Off
            }
            _ => SecurityState::Off,
        }
    }
}
