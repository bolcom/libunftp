//! Contains code pertaining to the FTP *data* channel

use super::{
    chancomms::{ControlChanMsg, DataChanMsg},
    tls::FtpsConfig,
};
use crate::server::session::SharedSession;
use crate::{
    auth::UserDetail,
    storage::{Error, ErrorKind, Metadata, StorageBackend},
};

use crate::server::chancomms::DataChanCmd;
#[cfg(unix)]
use std::{
    net::SocketAddr,
    os::fd::{AsRawFd, BorrowedFd, RawFd},
    sync::atomic::{AtomicU64, Ordering},
};
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_rustls::TlsAcceptor;

use crate::metrics;

#[derive(Debug)]
struct DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User>,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    pub user: Arc<Option<User>>,
    pub socket: TcpStream,
    pub control_msg_tx: Sender<ControlChanMsg>,
    pub storage: Arc<Storage>,
    pub cwd: PathBuf,
    pub ftps_mode: FtpsConfig,
    pub logger: slog::Logger,
    pub data_cmd_rx: Option<Receiver<DataChanCmd>>,
    pub data_abort_rx: Option<Receiver<()>>,
}

use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

/// Holds information about a socket processing a RETR command
#[cfg(unix)]
#[derive(Debug)]
pub struct RetrSocket {
    bytes: AtomicU64,
    fd: RawFd,
    peer: SocketAddr,
}

#[cfg(unix)]
impl RetrSocket {
    /// How many bytes have been written to the socket so far?
    ///
    /// Note that this tracks bytes written to the socket, not sent on the wire.
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        // Safe because we always destroy the RetrSocket when the MeasuringWriter drops
        #[allow(unsafe_code)]
        unsafe {
            BorrowedFd::borrow_raw(self.fd)
        }
    }

    fn new<W: AsRawFd>(w: &W) -> nix::Result<Self> {
        let fd = w.as_raw_fd();
        let ss: nix::sys::socket::SockaddrStorage = nix::sys::socket::getpeername(fd)?;
        let peer = if let Some(sin) = ss.as_sockaddr_in() {
            SocketAddr::V4((*sin).into())
        } else if let Some(sin6) = ss.as_sockaddr_in6() {
            SocketAddr::V6((*sin6).into())
        } else {
            return Err(nix::errno::Errno::EINVAL);
        };
        let bytes = Default::default();
        Ok(RetrSocket { bytes, fd, peer })
    }

    pub fn peer(&self) -> &SocketAddr {
        &self.peer
    }
}

/// Collection of all sockets currently serving RETR commands
#[cfg(unix)]
pub static RETR_SOCKETS: std::sync::RwLock<std::collections::BTreeMap<RawFd, RetrSocket>> = std::sync::RwLock::new(std::collections::BTreeMap::new());

#[cfg(unix)]
struct MeasuringWriter<W: AsRawFd> {
    writer: W,
    command: &'static str,
}
#[cfg(not(unix))]
struct MeasuringWriter<W> {
    writer: W,
    command: &'static str,
}

struct MeasuringReader<R> {
    reader: R,
    command: &'static str,
}

#[cfg(unix)]
impl<W: AsRawFd + AsyncWrite + Unpin> AsyncWrite for MeasuringWriter<W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();

        let result = Pin::new(&mut this.writer).poll_write(cx, buf);
        if let Poll::Ready(Ok(bytes_written)) = &result {
            let bw = *bytes_written as u64;
            RETR_SOCKETS
                .read()
                .unwrap()
                .get(&this.writer.as_raw_fd())
                .expect("TODO: better error handling")
                .bytes
                .fetch_add(bw, Ordering::Relaxed);
            metrics::inc_sent_bytes(*bytes_written, this.command);
        }

        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_shutdown(cx)
    }
}

#[cfg(not(unix))]
impl<W: AsyncWrite + Unpin> AsyncWrite for MeasuringWriter<W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();

        let result = Pin::new(&mut this.writer).poll_write(cx, buf);
        if let Poll::Ready(Ok(bytes_written)) = &result {
            let bw = *bytes_written as u64;
            metrics::inc_sent_bytes(*bytes_written, this.command);
        }

        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_shutdown(cx)
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for MeasuringReader<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.reader).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let bytes_read = buf.filled().len();
            metrics::inc_received_bytes(bytes_read, this.command);
        }
        result
    }
}

#[cfg(unix)]
impl<W: AsRawFd> MeasuringWriter<W> {
    fn new(writer: W, command: &'static str) -> MeasuringWriter<W> {
        let retr_socket = RetrSocket::new(&writer).expect("TODO: better error handling");
        RETR_SOCKETS.write().unwrap().insert(retr_socket.fd, retr_socket);
        Self { writer, command }
    }
}
#[cfg(not(unix))]
impl<W> MeasuringWriter<W> {
    fn new(writer: W, command: &'static str) -> MeasuringWriter<W> {
        Self { writer, command }
    }
}

#[cfg(unix)]
impl<W: AsRawFd> Drop for MeasuringWriter<W> {
    fn drop(&mut self) {
        if let Ok(mut guard) = RETR_SOCKETS.write() {
            guard.remove(&self.writer.as_raw_fd());
        }
    }
}

impl<R> MeasuringReader<R> {
    fn new(reader: R, command: &'static str) -> MeasuringReader<R> {
        Self { reader, command }
    }
}

impl<Storage, User> DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn execute(mut self, session_arc: SharedSession<Storage, User>) {
        let mut data_cmd_rx = self.data_cmd_rx.take().unwrap();
        let mut data_abort_rx = self.data_abort_rx.take().unwrap();
        let mut timeout_delay = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(5 * 60)));
        // TODO: Use configured timeout
        tokio::select! {
            Some(command) = data_cmd_rx.recv() => {
                let session = session_arc.lock().await;
                self.handle_incoming(DataChanMsg::ExternalCommand(command), session.start_pos).await;
            },
            Some(_) = data_abort_rx.recv() => {
                self.handle_incoming(DataChanMsg::Abort, 0).await;
            },
            _ = &mut timeout_delay => {
                slog::warn!(self.logger, "Data channel connection timed out");
            }
        };
        let mut session = session_arc.lock().await;
        session.data_busy = false;
    }

    #[tracing_attributes::instrument]
    async fn handle_incoming(self, incoming: DataChanMsg, start_pos: u64) {
        match incoming {
            DataChanMsg::Abort => {
                slog::info!(self.logger, "Data channel abort received");
            }
            DataChanMsg::ExternalCommand(command) => {
                let p = command.path().unwrap_or_default();
                slog::debug!(self.logger, "Data channel command received: {:?}", command; "path" => p);
                self.execute_command(command, start_pos).await;
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn execute_command(self, cmd: DataChanCmd, start_pos: u64) {
        match cmd {
            DataChanCmd::Retr { path } => {
                self.exec_retr(path, start_pos).await;
            }
            DataChanCmd::Stor { path } => {
                self.exec_stor(path, start_pos).await;
            }
            DataChanCmd::List { path, .. } => {
                self.exec_list_variant(path, ListCommand::List).await;
            }
            DataChanCmd::Nlst { path } => {
                self.exec_list_variant(path, ListCommand::Nlst).await;
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_retr(self, path: String, start_pos: u64) {
        let path_copy = path.clone();
        let path = self.cwd.join(path);
        let tx: Sender<ControlChanMsg> = self.control_msg_tx.clone();
        let mut output = Self::writer(self.socket, self.ftps_mode, "retr").await;

        let start_time = Instant::now();
        let result = self.storage.get_into((*self.user).as_ref().unwrap(), path, start_pos, &mut output).await;

        if let Err(err) = output.shutdown().await {
            match err.kind() {
                std::io::ErrorKind::BrokenPipe => {
                    slog::debug!(self.logger, "Output stream was already closed by peer after RETR: {:?}", err);
                }
                std::io::ErrorKind::NotConnected => {
                    slog::debug!(self.logger, "Output stream was already closed after RETR: {:?}", err);
                }
                _ => slog::warn!(self.logger, "Could not shutdown output stream after RETR: {:?}", err),
            }
        }

        let duration = start_time.elapsed();
        match result {
            Ok(bytes_copied) => {
                slog::info!(
                    self.logger,
                    "Successful RETR {:?}; Duration {}; Bytes copied {}; Transfer speed {}; start_pos={}",
                    &path_copy,
                    HumanDuration(duration),
                    HumanBytes(bytes_copied),
                    TransferSpeed(bytes_copied as f64 / duration.as_secs_f64()),
                    start_pos,
                );

                // only register transfer of a single file transfer
                if start_pos == 0 {
                    metrics::inc_transferred("retr", "success");
                }

                if let Err(err) = tx
                    .send(ControlChanMsg::SentData {
                        bytes: bytes_copied,
                        path: path_copy,
                    })
                    .await
                {
                    slog::error!(self.logger, "Could not notify control channel of successful RETR: {:?}", err);
                }
            }
            Err(err) => {
                let io_error_kind = err.get_io_error().map(|e| e.kind());

                if io_error_kind == Some(std::io::ErrorKind::BrokenPipe) {
                    if start_pos == 0 {
                        slog::warn!(
                            self.logger,
                            "Client halted RETR transfer (BrokenPipe). Certain FTP clients may do this to download file sections separately, in which case RESTarts may occur and will be logged at DEBUG level. Refer to your FTP client's documentation if this causes issues. Path {:?}; Duration {} (number of bytes copied unknown).",
                            &path_copy,
                            HumanDuration(duration)
                        );
                    } else {
                        slog::debug!(
                            self.logger,
                            "RETR transfer stopped by client (BrokenPipe). Remember, this could be standard for some FTP clients. Path {:?}; Duration {} (number of bytes copied unknown); start_pos {}",
                            &path_copy,
                            HumanDuration(duration),
                            start_pos
                        );
                    }
                } else {
                    slog::warn!(
                        self.logger,
                        "Error during RETR {:?} transfer after {}: {:?}; start_pos={}",
                        &path_copy,
                        HumanDuration(duration),
                        err,
                        start_pos
                    );
                }

                // only register transfer errors for a single file transfer once
                if start_pos == 0 {
                    categorize_and_register_error(&self.logger, &err, "retr");
                }

                if let Err(err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(self.logger, "Could not notify control channel of error with RETR: {:?}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_stor(self, path: String, start_pos: u64) {
        let path_copy = path.clone();
        let path = self.cwd.join(path);
        let tx = self.control_msg_tx.clone();

        let start_time = Instant::now();
        let put_result = self
            .storage
            .put(
                (*self.user).as_ref().unwrap(),
                Self::reader(self.socket, self.ftps_mode, "stor").await,
                path,
                start_pos,
            )
            .await;
        let duration = start_time.elapsed();

        match put_result {
            Ok(bytes) => {
                slog::info!(
                    self.logger,
                    "Successful STOR {:?}; Duration {}; Bytes copied {}; Transfer speed {}; start_pos={}",
                    &path_copy,
                    HumanDuration(duration),
                    HumanBytes(bytes),
                    TransferSpeed(bytes as f64 / duration.as_secs_f64()),
                    start_pos,
                );

                // only register transfer of a single file transfer
                if start_pos == 0 {
                    metrics::inc_transferred("stor", "success");
                }

                if let Err(err) = tx.send(ControlChanMsg::WrittenData { bytes, path: path_copy }).await {
                    slog::error!(self.logger, "Could not notify control channel of successful STOR: {:?}", err);
                }
            }
            Err(err) => {
                slog::warn!(self.logger, "Error during STOR transfer after {}: {:?}", HumanDuration(duration), err);

                // only register transfer errors for a single file transfer once
                if start_pos == 0 {
                    categorize_and_register_error(&self.logger, &err, "stor");
                }

                if let Err(err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with STOR: {:?}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn exec_list_variant(self, path: Option<String>, command: ListCommand) {
        let path = self.resolve_path(path);
        let tx = self.control_msg_tx.clone();
        let mut output = Self::writer(self.socket, self.ftps_mode.clone(), command.as_lower_str()).await;

        let start_time = Instant::now();

        let list_result = match command {
            ListCommand::List => self.storage.list_fmt((*self.user).as_ref().unwrap(), path.clone()).await,
            ListCommand::Nlst => self
                .storage
                .nlst((*self.user).as_ref().unwrap(), path.clone())
                .await
                .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e)),
        };

        match list_result {
            Ok(cursor) => {
                slog::debug!(self.logger, "Copying future for {}", command.as_str());
                let mut input = cursor;
                let result = tokio::io::copy(&mut input, &mut output).await;

                if let Err(err) = output.shutdown().await {
                    match err.kind() {
                        std::io::ErrorKind::BrokenPipe => {
                            slog::debug!(self.logger, "Output stream was already closed by peer after {}: {:?}", command.as_str(), err);
                        }
                        std::io::ErrorKind::NotConnected => {
                            slog::debug!(self.logger, "Output stream was already closed after {}: {:?}", command.as_str(), err);
                        }
                        _ => slog::warn!(self.logger, "Could not shutdown output stream after {}: {:?}", command.as_str(), err),
                    }
                }
                let duration = start_time.elapsed();

                match result {
                    Ok(bytes) => {
                        slog::info!(
                            self.logger,
                            "Successful LIST {:?}; Duration {}; Bytes copied {}; Transfer speed {}",
                            path,
                            HumanDuration(duration),
                            HumanBytes(bytes),
                            TransferSpeed(bytes as f64 / duration.as_secs_f64()),
                        );
                        metrics::inc_transferred(command.as_lower_str(), "success");
                        if let Err(err) = tx.send(ControlChanMsg::DirectorySuccessfullyListed).await {
                            slog::error!(self.logger, "Could not notify control channel of error with {}: {:?}", command.as_str(), err);
                        }
                    }
                    Err(e) => {
                        let duration = start_time.elapsed();
                        slog::warn!(
                            self.logger,
                            "Failed to send directory list for path {:?} ({} command) after {}: {:?}",
                            path,
                            command.as_str(),
                            HumanDuration(duration),
                            e,
                        );

                        let err = Error::from(e);
                        categorize_and_register_error(&self.logger, &err, command.as_lower_str());
                    }
                }
            }
            Err(err) => {
                let duration = start_time.elapsed();

                slog::warn!(
                    self.logger,
                    "Failed to retrieve directory list for path {:?} ({} command) from storage backend after {}: {:?}",
                    path,
                    command.as_str(),
                    HumanDuration(duration),
                    err,
                );

                categorize_and_register_error(&self.logger, &err, command.as_lower_str());

                if let Err(err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with {}: {:?}", command.as_str(), err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn writer(socket: TcpStream, ftps_mode: FtpsConfig, command: &'static str) -> Box<dyn AsyncWrite + Send + Unpin + Sync> {
        match ftps_mode {
            FtpsConfig::Off => Box::new(MeasuringWriter::new(socket, command)) as Box<dyn AsyncWrite + Send + Unpin + Sync>,
            FtpsConfig::Building { .. } => panic!("Illegal state"),
            FtpsConfig::On { tls_config } => {
                let io = async move {
                    let acceptor: TlsAcceptor = tls_config.into();
                    let tls_stream = acceptor.accept(socket).await.unwrap();
                    MeasuringWriter::new(tls_stream, command)
                }
                .await;
                Box::new(io) as Box<dyn AsyncWrite + Send + Unpin + Sync>
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn reader(socket: TcpStream, ftps_mode: FtpsConfig, command: &'static str) -> Box<dyn AsyncRead + Send + Unpin + Sync> {
        match ftps_mode {
            FtpsConfig::Off => Box::new(MeasuringReader::new(socket, command)) as Box<dyn AsyncRead + Send + Unpin + Sync>,
            FtpsConfig::Building { .. } => panic!("Illegal state"),
            FtpsConfig::On { tls_config } => {
                let io = async move {
                    let acceptor: TlsAcceptor = tls_config.into();
                    let tls_stream = acceptor.accept(socket).await.unwrap();
                    MeasuringReader::new(tls_stream, command)
                }
                .await;
                Box::new(io) as Box<dyn AsyncRead + Send + Unpin + Sync>
            }
        }
    }

    fn resolve_path(&self, path: Option<String>) -> PathBuf {
        match path {
            Some(path) => {
                if path == "." {
                    self.cwd.clone()
                } else {
                    self.cwd.join(path)
                }
            }
            None => self.cwd.clone(),
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
pub async fn spawn_processing<Storage, User>(logger: slog::Logger, session_arc: SharedSession<Storage, User>, mut socket: TcpStream)
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
                let controlchan_ip = session.source.ip();
                if controlchan_ip != datachan_addr.ip() {
                    if let Err(err) = socket.shutdown().await {
                        slog::error!(
                            logger,
                            "Couldn't close datachannel for IP ({}) that does not match the IP({}) of the control channel: {:?}",
                            datachan_addr.ip(),
                            controlchan_ip,
                            err
                        )
                    } else {
                        slog::warn!(
                            logger,
                            "Closing datachannel for IP ({}) that does not match the IP({}) of the control channel.",
                            datachan_addr.ip(),
                            controlchan_ip
                        )
                    }
                    return;
                }
            }
            Err(err) => {
                slog::error!(logger, "Couldn't determine data channel address: {:?}", err);
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
        let ftps_mode = if session.data_tls { session.ftps_config.clone() } else { FtpsConfig::Off };
        let command_executor = DataCommandExecutor {
            user: session.user.clone(),
            socket,
            control_msg_tx,
            storage: Arc::clone(&session.storage),
            cwd: session.cwd.clone(),
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

use std::time::Duration;

struct HumanDuration(Duration);

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_secs = self.0.as_secs();

        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = self.0.subsec_millis();

        if hours > 0 {
            write!(f, "{}h {}m {}s {}ms", hours, minutes, seconds, millis)
        } else if minutes > 0 {
            write!(f, "{}m {}s {}ms", minutes, seconds, millis)
        } else if seconds > 0 {
            write!(f, "{}s {}ms", seconds, millis)
        } else {
            write!(f, "{}ms", millis)
        }
    }
}

struct HumanBytes(u64);

impl fmt::Display for HumanBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const KIB: u64 = 1024;
        const MIB: u64 = KIB * 1024;
        const GIB: u64 = MIB * 1024;
        const TIB: u64 = GIB * 1024;

        if self.0 >= TIB {
            write!(f, "{:.2} TiB", (self.0 as f64) / (TIB as f64))
        } else if self.0 >= GIB {
            write!(f, "{:.2} GiB", (self.0 as f64) / (GIB as f64))
        } else if self.0 >= MIB {
            write!(f, "{:.2} MiB", (self.0 as f64) / (MIB as f64))
        } else if self.0 >= KIB {
            write!(f, "{:.2} KiB", (self.0 as f64) / (KIB as f64))
        } else {
            write!(f, "{} B", self.0)
        }
    }
}

struct TransferSpeed(f64);

impl fmt::Display for TransferSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kb_per_second = self.0 / 1024.0;
        if kb_per_second < 1.0 {
            return write!(f, "{:.2} B/s", self.0);
        }

        let mb_per_second = kb_per_second / 1024.0;
        if mb_per_second < 1.0 {
            return write!(f, "{:.2} KB/s", kb_per_second);
        }

        let gb_per_second = mb_per_second / 1024.0;
        if gb_per_second < 1.0 {
            return write!(f, "{:.2} MB/s", mb_per_second);
        }

        write!(f, "{:.2} GB/s", gb_per_second)
    }
}

// Collapse the StorageError kind into a client-error, server-error or unknown-error.
// The PermissionDenied is seperated because it depends on specifics whether it is a server or client error
// Unknown errors should not happen but need to be handled
fn categorize_and_register_error(logger: &slog::Logger, err: &Error, command: &'static str) {
    match err.kind() {
        ErrorKind::PermanentFileNotAvailable => metrics::inc_transferred(command, "client-error"),
        ErrorKind::TransientFileNotAvailable | ErrorKind::LocalError => metrics::inc_transferred(command, "server-error"),
        ErrorKind::PermissionDenied => metrics::inc_transferred(command, "permission-error"),
        ErrorKind::ConnectionClosed => {
            if let Some(io_error) = err.get_io_error() {
                match io_error.kind() {
                    std::io::ErrorKind::ConnectionReset => metrics::inc_transferred(command, "client-interrupted"),
                    std::io::ErrorKind::BrokenPipe => {
                        // Clients like Cyberduck appear to close the connection prematurely for chunked downloading, generating many "errors"
                        if command != "retr" {
                            metrics::inc_transferred(command, "client-interrupted");
                        }
                    }
                    std::io::ErrorKind::ConnectionAborted => metrics::inc_transferred(command, "network-error"), // Could be a network issue
                    _ => {
                        slog::debug!(logger, "Unmapped ConnectionClosed io error: {:?}", io_error);
                        metrics::inc_transferred(command, "server-error")
                    }
                }
            }
        }
        _ => {
            slog::debug!(logger, "Unmapped error: {:?}", err);
            metrics::inc_transferred(command, "unknown-error")
        }
    }
}

#[derive(Debug)]
enum ListCommand {
    List,
    Nlst,
}

impl ListCommand {
    fn as_str(&self) -> &'static str {
        match self {
            ListCommand::List => "LIST",
            ListCommand::Nlst => "NLST",
        }
    }
    fn as_lower_str(&self) -> &'static str {
        match self {
            ListCommand::List => "list",
            ListCommand::Nlst => "nlst",
        }
    }
}
