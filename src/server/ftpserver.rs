pub mod options;

use crate::{
    auth::{anonymous::AnonymousAuthenticator, Authenticator, DefaultUser, UserDetail},
    server::{
        proxy_protocol::{get_peer_from_proxy_header, ConnectionTuple, ProxyMode, ProxyProtocolSwitchboard},
        session::SharedSession,
    },
    storage::{filesystem::Filesystem, Metadata, StorageBackend},
};
use options::{PassiveHost, DEFAULT_GREETING, DEFAULT_IDLE_SESSION_TIMEOUT_SECS};

use super::{
    chancomms::{InternalMsg, ProxyLoopMsg, ProxyLoopReceiver, ProxyLoopSender},
    controlchan::{spawn_loop, LoopConfig},
    datachan::spawn_processing,
    tls::FTPSConfig,
};
use crate::server::ftpserver::options::FtpsRequired;
use crate::server::Reply;
use futures::{channel::mpsc::channel, SinkExt, StreamExt};
use slog::*;
use std::{
    fmt::Debug,
    net::{IpAddr, Shutdown, SocketAddr},
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

/// An instance of an FTP(S) server. It aggregates an [`Authenticator`] that will be used
/// for authentication, and a [`StorageBackend`] that will be used as the virtual file system.
///
/// The server can be started with the `listen` method.
///
/// # Example
///
/// ```rust
/// use libunftp::Server;
/// use tokio::runtime::Runtime;
///
/// let mut rt = Runtime::new().unwrap();
/// let server = Server::new_with_fs_root("/srv/ftp");
/// rt.spawn(server.listen("127.0.0.1:2121"));
/// // ...
/// drop(rt);
/// ```
///
/// [`Authenticator`]: auth/trait.Authenticator.html
/// [`StorageBackend`]: storage/trait.StorageBackend.html
pub struct Server<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail,
{
    storage: Box<dyn (Fn() -> S) + Send + Sync>,
    greeting: &'static str,
    authenticator: Arc<dyn Authenticator<U>>,
    passive_ports: Range<u16>,
    passive_host: PassiveHost,
    collect_metrics: bool,
    ftps_mode: FTPSConfig,
    ftps_required: FtpsRequired,
    idle_session_timeout: std::time::Duration,
    proxy_protocol_mode: ProxyMode,
    proxy_protocol_switchboard: Option<ProxyProtocolSwitchboard<S, U>>,
    logger: slog::Logger,
}

impl<S, U> Debug for Server<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("greeting", &self.greeting)
            .field("authenticator", &self.authenticator)
            .field("passive_ports", &self.passive_ports)
            .field("collect_metrics", &self.collect_metrics)
            .field("ftps_mode", &self.ftps_mode)
            .field("idle_session_timeout", &self.idle_session_timeout)
            .field("proxy_protocol_mode", &self.proxy_protocol_mode)
            .field("proxy_protocol_switchboard", &self.proxy_protocol_switchboard)
            .finish()
    }
}

impl<S, U> Server<S, U>
where
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    U: UserDetail + 'static,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`] generator and an [`AnonymousAuthenticator`]
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`AnonymousAuthenticator`]: ../auth/struct.AnonymousAuthenticator.html
    pub fn new(sbe_generator: Box<dyn (Fn() -> S) + Send + Sync>) -> Self
    where
        AnonymousAuthenticator: Authenticator<U>,
    {
        Self::new_with_authenticator(sbe_generator, Arc::new(AnonymousAuthenticator {}))
    }

    /// Construct a new [`Server`] with the given [`StorageBackend`] and [`Authenticator`]. The other parameters will be set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn new_with_authenticator(s: Box<dyn (Fn() -> S) + Send + Sync>, authenticator: Arc<dyn Authenticator<U> + Send + Sync>) -> Self {
        Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator,
            passive_ports: options::DEFAULT_PASSIVE_PORTS,
            passive_host: options::DEFAULT_PASSIVE_HOST,
            ftps_mode: FTPSConfig::Off,
            collect_metrics: false,
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
            proxy_protocol_mode: ProxyMode::Off,
            proxy_protocol_switchboard: Option::None,
            logger: slog::Logger::root(slog_stdlog::StdLog {}.fuse(), slog::o!()),
            ftps_required: options::DEFAULT_FTPS_REQUIRE,
        }
    }

    /// Set the [`Authenticator`] that will be used for authentication.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::{auth, auth::AnonymousAuthenticator, Server};
    /// use std::sync::Arc;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::new_with_fs_root("/tmp")
    ///                  .authenticator(Arc::new(auth::AnonymousAuthenticator{}));
    /// ```
    ///
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn authenticator(mut self, authenticator: Arc<dyn Authenticator<U> + Send + Sync>) -> Self {
        self.authenticator = authenticator;
        self
    }

    /// Configures the path to the certificates file (DER-formatted PKCS #12 archive) and the
    /// associated password for the archive in order to configure FTPS.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .ftps("/srv/unftp/server.certs", "/srv/unftp/server.key");
    /// ```
    pub fn ftps<P: Into<PathBuf>>(mut self, certs_file: P, key_file: P) -> Self {
        self.ftps_mode = FTPSConfig::On {
            certs_file: certs_file.into(),
            key_file: key_file.into(),
        };
        self
    }

    /// Configures whether client connections may use plaintext mode or not.
    pub fn ftps_required(mut self, option: impl Into<FtpsRequired>) -> Self {
        self.ftps_required = option.into();
        self
    }

    /// Set the greeting that will be sent to the client after connecting.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::new_with_fs_root("/tmp").greeting("Welcome to my FTP Server");
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::new_with_fs_root("/tmp");
    /// server.greeting("Welcome to my FTP Server");
    /// ```
    pub fn greeting(mut self, greeting: &'static str) -> Self {
        self.greeting = greeting;
        self
    }

    /// Set the idle session timeout in seconds. The default is 600 seconds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::new_with_fs_root("/tmp").idle_session_timeout(600);
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::new_with_fs_root("/tmp");
    /// server.idle_session_timeout(600);
    /// ```
    pub fn idle_session_timeout(mut self, secs: u64) -> Self {
        self.idle_session_timeout = Duration::from_secs(secs);
        self
    }

    /// Sets the structured logger ([slog](https://crates.io/crates/slog)::Logger) to use
    pub fn logger<L: Into<Option<slog::Logger>>>(mut self, logger: L) -> Self {
        self.logger = logger.into().unwrap_or_else(|| slog::Logger::root(slog_stdlog::StdLog {}.fuse(), slog::o!()));
        self
    }

    /// Enables the collection of prometheus metrics.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::new_with_fs_root("/tmp").metrics();
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::new_with_fs_root("/tmp");
    /// server.metrics();
    /// ```
    pub fn metrics(mut self) -> Self {
        self.collect_metrics = true;
        self
    }

    /// Specifies how the IP address that libunftp will advertise in response to the PASV command is
    /// determined.
    ///
    /// # Examples
    ///
    /// Using a fixed IP specified as a numeric array:
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .passive_host([127,0,0,1]);
    /// ```
    /// Or the same but more explicitly:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use std::net::Ipv4Addr;
    ///
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .passive_host(options::PassiveHost::IP(Ipv4Addr::new(127, 0, 0, 1)));
    /// ```
    ///
    /// To determine the passive IP from the incoming control connection:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    ///
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .passive_host(options::PassiveHost::FromConnection);
    /// ```
    ///
    /// Get the IP by resolving a DNS name:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    ///
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .passive_host("ftp.myserver.org");
    /// ```
    pub fn passive_host<H: Into<PassiveHost>>(mut self, host_option: H) -> Self {
        self.passive_host = host_option.into();
        self
    }

    /// Sets the range of passive ports that we'll use for passive connections.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let server = Server::new_with_fs_root("/tmp")
    ///              .passive_ports(49152..65535);
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::new_with_fs_root("/tmp");
    /// server.passive_ports(49152..65535);
    /// ```
    pub fn passive_ports(mut self, range: Range<u16>) -> Self {
        self.passive_ports = range;
        self
    }

    /// Enables PROXY protocol mode.
    ///
    /// If you use a proxy such as haproxy or nginx, you can enable
    /// the PROXY protocol
    /// (https://www.haproxy.org/download/1.8/doc/proxy-protocol.txt).
    ///
    /// Configure your proxy to enable PROXY protocol encoding for
    /// control and data external listening ports, forwarding these
    /// connections to the libunFTP listening port in proxy protocol
    /// mode.
    ///
    /// In PROXY protocol mode, libunftp receives both control and
    /// data connections on the listening port. It then distinguishes
    /// control and data connections by comparing the original
    /// destination port (extracted from the PROXY header) with the
    /// port specified as the `external_control_port` parameter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::new_with_fs_root("/tmp").proxy_protocol_mode(2121);
    /// ```
    pub fn proxy_protocol_mode(mut self, external_control_port: u16) -> Self {
        self.proxy_protocol_mode = external_control_port.into();
        self.proxy_protocol_switchboard = Some(ProxyProtocolSwitchboard::new(self.logger.clone(), self.passive_ports.clone()));
        self
    }

    /// Runs the main ftp process asynchronously. Should be started in a async runtime context.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use tokio::runtime::Runtime;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let server = Server::new_with_fs_root("/srv/ftp");
    /// rt.spawn(server.listen("127.0.0.1:2121"));
    /// // ...
    /// drop(rt);
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics when called with invalid addresses or when the process is unable to
    /// `bind()` to the address.
    #[tracing_attributes::instrument]
    pub async fn listen<T: Into<String> + Debug>(self, bind_address: T) {
        match self.proxy_protocol_mode {
            ProxyMode::On { external_control_port } => self.listen_proxy_protocol_mode(bind_address, external_control_port).await,
            ProxyMode::Off => self.listen_normal_mode(bind_address).await,
        }
    }

    #[tracing_attributes::instrument]
    async fn listen_normal_mode<T: Into<String> + Debug>(self, bind_address: T) {
        // TODO: Propagate errors to caller instead of doing unwraps.
        let addr: std::net::SocketAddr = bind_address.into().parse().unwrap();
        let mut listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        loop {
            let (tcp_stream, socket_addr) = listener.accept().await.unwrap();
            slog::info!(self.logger, "Incoming control channel connection from {:?}", socket_addr);
            let params: LoopConfig<S, U> = (&self).into();
            let result = spawn_loop::<S, U>(params, tcp_stream, None, None).await;
            if result.is_err() {
                slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", result.err().unwrap())
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn listen_proxy_protocol_mode<T: Into<String> + Debug>(mut self, bind_address: T, external_control_port: u16) {
        // TODO: Propagate errors to caller instead of doing unwraps.
        let addr: std::net::SocketAddr = bind_address.into().parse().unwrap();
        let mut listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        // this callback is used by all sessions, basically only to
        // request for a passive listening port.
        let (proxyloop_msg_tx, mut proxyloop_msg_rx): (ProxyLoopSender<S, U>, ProxyLoopReceiver<S, U>) = channel(1);

        let mut incoming = listener.incoming();

        loop {
            // The 'proxy loop' handles two kinds of events:
            // - incoming tcp connections originating from the proxy
            // - channel messages originating from PASV, to handle the passive listening port

            tokio::select! {

                Some(tcp_stream) = incoming.next() => {
                    let mut tcp_stream = tcp_stream.unwrap();
                    let socket_addr = tcp_stream.peer_addr();

                    slog::info!(self.logger, "Incoming proxy connection from {:?}", socket_addr);
                    let connection = match get_peer_from_proxy_header(&mut tcp_stream).await {
                        Ok(v) => v,
                        Err(e) => {
                            slog::warn!(self.logger, "proxy protocol decode error: {:?}", e);
                            continue;
                        }
                    };

                    // Based on the proxy protocol header, and the configured control port number,
                    // we differentiate between connections for the control channel,
                    // and connections for the data channel.
                    if connection.to_port == external_control_port {
                        let socket_addr = SocketAddr::new(connection.from_ip, connection.from_port);
                        slog::info!(self.logger, "Connection from {:?} is a control connection", socket_addr);
                        let params: LoopConfig<S,U> = (&self).into();
                        let result = spawn_loop::<S,U>(params, tcp_stream, Some(connection), Some(proxyloop_msg_tx.clone())).await;
                        if result.is_err() {
                            slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", result.err().unwrap())
                        }
                    } else {
                        // handle incoming data connections
                        slog::info!(self.logger, "Connection from {:?} is a data connection: {:?}, {}", socket_addr, self.passive_ports, connection.to_port);
                        if !self.passive_ports.contains(&connection.to_port) {
                            slog::warn!(self.logger, "Incoming proxy connection going to unconfigured port! This port is not configured as a passive listening port: port {} not in passive port range {:?}", connection.to_port, self.passive_ports);
                            tcp_stream.shutdown(Shutdown::Both).unwrap();
                            continue;
                        }
                        self.dispatch_data_connection(tcp_stream, connection).await;
                    }
                },
                Some(msg) = proxyloop_msg_rx.next() => {
                    match msg {
                        ProxyLoopMsg::AssignDataPortCommand (session_arc) => {
                            self.select_and_register_passive_port(session_arc).await;
                        },
                    }
                },
            };
        }
    }

    // this function finds (by hashing <srcip>.<dstport>) the session
    // that requested this data channel connection in the proxy
    // protocol switchboard hashmap, and then calls the
    // spawn_data_processing function with the tcp_stream
    #[tracing_attributes::instrument]
    async fn dispatch_data_connection(&mut self, tcp_stream: tokio::net::TcpStream, connection: ConnectionTuple) {
        if let Some(switchboard) = &mut self.proxy_protocol_switchboard {
            match switchboard.get_session_by_incoming_data_connection(&connection).await {
                Some(session) => {
                    let mut session = session.lock().await;
                    let tx_some = session.control_msg_tx.clone();
                    let s = session.username.as_ref().cloned().unwrap_or_else(|| String::from("unknown"));
                    if let Some(tx) = tx_some {
                        let logger = self.logger.new(slog::o!("username" => s));
                        spawn_processing(logger, &mut session, tcp_stream, tx);
                        switchboard.unregister(&connection);
                    }
                }
                None => {
                    slog::warn!(self.logger, "Unexpected connection ({:?})", connection);
                    tcp_stream.shutdown(Shutdown::Both).unwrap();
                    return;
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn select_and_register_passive_port(&mut self, session_arc: SharedSession<S, U>) {
        slog::info!(self.logger, "Received internal message to allocate data port");
        // 1. reserve a port
        // 2. put the session_arc and tx in the hashmap with srcip+dstport as key
        // 3. put expiry time in the LIFO list
        // 4. send reply to client: "Entering Passive Mode ({},{},{},{},{},{})"

        let mut reserved_port: u16 = 0;
        if let Some(switchboard) = &mut self.proxy_protocol_switchboard {
            let port = switchboard.reserve_next_free_port(session_arc.clone()).await.unwrap();
            slog::info!(self.logger, "Reserving data port: {:?}", port);
            reserved_port = port
        }
        let session = session_arc.lock().await;
        if let Some(conn) = session.control_connection_info {
            let conn_ip = match conn.to_ip {
                IpAddr::V4(ref ip) => ip,
                IpAddr::V6(_) => panic!("Won't happen since PASV only does IP V4."),
            };

            let reply: Reply = super::controlchan::commands::make_pasv_reply(self.passive_host.clone(), conn_ip, reserved_port).await;

            let tx_some = session.control_msg_tx.clone();
            if let Some(tx) = tx_some {
                let mut tx = tx.clone();
                tx.send(InternalMsg::CommandChannelReply(reply)).await.unwrap();
            }
        }
    }
}

impl Server<Filesystem, DefaultUser> {
    /// Create a new `Server` with the given filesystem root.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let server = Server::new_with_fs_root("/srv/ftp");
    /// ```
    pub fn new_with_fs_root<P: Into<PathBuf> + Send + 'static>(path: P) -> Self {
        let p = path.into();
        Server::new(Box::new(move || {
            let p = &p.clone();
            Filesystem::new(p)
        }))
    }
}

impl<U> Server<Filesystem, U>
where
    U: UserDetail + 'static,
{
    /// Create a new `Server` using the filesystem backend and the specified authenticator
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use libunftp::auth::AnonymousAuthenticator;
    /// use std::sync::Arc;
    ///
    /// let server = Server::new_with_fs_and_auth("/srv/ftp", Arc::new(AnonymousAuthenticator{}));
    /// ```
    pub fn new_with_fs_and_auth<P: Into<PathBuf> + Send + 'static>(path: P, authenticator: Arc<dyn Authenticator<U> + Send + Sync>) -> Self {
        let p = path.into();
        Server::new_with_authenticator(
            Box::new(move || {
                let p = &p.clone();
                Filesystem::new(p)
            }),
            authenticator,
        )
    }
}

impl<S, U> From<&Server<S, U>> for LoopConfig<S, U>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    fn from(server: &Server<S, U>) -> Self {
        LoopConfig {
            authenticator: server.authenticator.clone(),
            storage: (server.storage)(),
            ftps_config: server.ftps_mode.clone(),
            collect_metrics: server.collect_metrics,
            greeting: server.greeting,
            idle_session_timeout: server.idle_session_timeout,
            passive_ports: server.passive_ports.clone(),
            passive_host: server.passive_host.clone(),
            logger: server.logger.new(slog::o!()),
            ftps_required: server.ftps_required,
        }
    }
}
