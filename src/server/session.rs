//! The session module implements per-connection session handling and currently also
//! implements the handling for the *data* channel.

use super::chancomms::{DataCommand, InternalMsg};
use super::controlchan::command::Command;
use super::datachan::DataCommandExecutor;
use super::proxy_protocol::ConnectionTuple;
use crate::metrics;
use crate::storage;

use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::{info, warn};
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
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    pub user: Arc<Option<U>>,
    pub username: Option<String>,
    pub storage: Arc<S>,
    pub data_cmd_tx: Option<Sender<Command>>,
    pub data_cmd_rx: Option<Receiver<Command>>,
    pub data_abort_tx: Option<Sender<()>>,
    pub data_abort_rx: Option<Receiver<()>>,
    pub internal_msg_tx: Option<Sender<InternalMsg>>,
    pub control_connection: Option<ConnectionTuple>,
    pub cwd: std::path::PathBuf,
    pub rename_from: Option<PathBuf>,
    pub state: SessionState,
    pub certs_file: Option<PathBuf>,
    pub certs_password: Option<String>,
    // True if the command channel is in secure mode
    pub cmd_tls: bool,
    // True if the data channel is in secure mode.
    pub data_tls: bool,
    pub collect_metrics: bool,
    // The starting byte for a STOR or RETR command. Set by the _Restart of Interrupted Transfer (REST)_
    // command to support resume functionality.
    pub start_pos: u64,
}

impl<S, U: Send + Sync + 'static> Session<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    pub(super) fn new(storage: Arc<S>) -> Self {
        Session {
            user: Arc::new(None),
            username: None,
            storage,
            data_cmd_tx: None,
            data_cmd_rx: None,
            data_abort_tx: None,
            data_abort_rx: None,
            internal_msg_tx: None,
            control_connection: None,
            cwd: "/".into(),
            rename_from: None,
            state: SessionState::New,
            certs_file: Option::None,
            certs_password: Option::None,
            cmd_tls: false,
            data_tls: false,
            collect_metrics: false,
            start_pos: 0,
        }
    }

    pub(super) fn ftps(mut self, certs_file: Option<PathBuf>, password: Option<String>) -> Self {
        self.certs_file = certs_file;
        self.certs_password = password;
        self
    }

    pub(super) fn metrics(mut self, collect_metrics: bool) -> Self {
        if collect_metrics {
            metrics::inc_session();
        }
        self.collect_metrics = collect_metrics;
        self
    }

    /// Processing for the data connection. This will spawn a new async task with the actual processing.
    ///
    /// socket: the data socket we'll be working with
    /// tls: tells if this should be a TLS connection
    /// tx: channel to send the result of our operation to the control process
    //
    // TODO: This doesn't really belong here, move to datachan.rs
    pub(super) fn spawn_data_processing(&mut self, socket: tokio::net::TcpStream, tx: Sender<InternalMsg>) {
        let mut data_cmd_rx = self.data_cmd_rx.take().unwrap().fuse();
        let mut data_abort_rx = self.data_abort_rx.take().unwrap().fuse();
        let tls = self.data_tls;
        let command_executor = DataCommandExecutor {
            user: self.user.clone(),
            socket,
            tls,
            tx,
            storage: Arc::clone(&self.storage),
            cwd: self.cwd.clone(),
            start_pos: self.start_pos,
            identity_file: if tls { Some(self.certs_file.clone().unwrap()) } else { None },
            identity_password: if tls { Some(self.certs_password.clone().unwrap()) } else { None },
        };

        tokio::spawn(async move {
            let mut timeout_delay = tokio::time::delay_for(std::time::Duration::from_secs(5 * 60));
            // TODO: Use configured timeout
            tokio::select! {
                Some(command) = data_cmd_rx.next() => {
                    Self::handle_incoming(DataCommand::ExternalCommand(command), command_executor).await;
                },
                Some(_) = data_abort_rx.next() => {
                    Self::handle_incoming(DataCommand::Abort, command_executor).await;
                },
                _ = &mut timeout_delay => {
                    info!("Connection timed out");
                    return;
                }
            };

            // This probably happened because the control channel was closed before we got here
            warn!("Nothing received");
        });
    }

    //
    // TODO: This doesn't really belong here, move to datachan.rs
    async fn handle_incoming(incoming: DataCommand, command_executor: DataCommandExecutor<S, U>) {
        match incoming {
            DataCommand::Abort => {
                info!("Abort received");
            }
            DataCommand::ExternalCommand(command) => {
                info!("Data command received");
                command_executor.execute(command).await;
            }
        }
    }
}

impl<S, U: Send + Sync> Drop for Session<S, U>
where
    S: storage::StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn drop(&mut self) {
        if self.collect_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
