pub mod error;
pub mod options;

use super::{
    chancomms::{ControlChanMsg, ProxyLoopMsg, ProxyLoopReceiver, ProxyLoopSender},
    controlchan,
    datachan::spawn_processing,
    ftpserver::{error::ServerError, options::FtpsRequired, options::SiteMd5},
    tls::FtpsConfig,
};
use crate::{
    auth::{anonymous::AnonymousAuthenticator, Authenticator, UserDetail},
    server::{
        proxy_protocol::{get_peer_from_proxy_header, ConnectionTuple, ProxyMode, ProxyProtocolSwitchboard},
        session::SharedSession,
        Reply,
    },
    storage::{Metadata, StorageBackend},
};

use crate::options::{FtpsClientAuth, TlsFlags};
use crate::server::tls;
use futures::{channel::mpsc::channel, SinkExt};
use options::{PassiveHost, DEFAULT_GREETING, DEFAULT_IDLE_SESSION_TIMEOUT_SECS};
use slog::*;
use std::{fmt::Debug, net::IpAddr, ops::Range, path::PathBuf, sync::Arc, time::Duration};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

/// An instance of an FTP(S) server. It aggregates an [`Authenticator`](crate::auth::Authenticator)
/// implementation that will be used for authentication, and a [`StorageBackend`](crate::storage::StorageBackend)
/// implementation that will be used as the virtual file system.
///
/// The server can be started with the [`listen`](crate::Server::listen()) method.
///
/// # Example
///
/// ```rust
/// use libunftp::Server;
/// use unftp_sbe_fs::ServerExt;
/// use tokio::runtime::Runtime;
///
/// let mut rt = Runtime::new().unwrap();
/// let server = Server::with_fs("/srv/ftp");
/// rt.spawn(server.listen("127.0.0.1:2121"));
/// // ...
/// drop(rt);
/// ```
///
/// [`Authenticator`]: auth::Authenticator
/// [`StorageBackend`]: storage/trait.StorageBackend.html
pub struct Server<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    storage: Box<dyn (Fn() -> Storage) + Send + Sync>,
    greeting: &'static str,
    authenticator: Arc<dyn Authenticator<User>>,
    passive_ports: Range<u16>,
    passive_host: PassiveHost,
    collect_metrics: bool,
    ftps_mode: FtpsConfig,
    ftps_required_control_chan: FtpsRequired,
    ftps_required_data_chan: FtpsRequired,
    ftps_tls_flags: TlsFlags,
    ftps_client_auth: FtpsClientAuth,
    ftps_trust_store: PathBuf,
    idle_session_timeout: std::time::Duration,
    proxy_protocol_mode: ProxyMode,
    proxy_protocol_switchboard: Option<ProxyProtocolSwitchboard<Storage, User>>,
    logger: slog::Logger,
    sitemd5: SiteMd5,
}

impl<Storage, User> Debug for Server<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("authenticator", &self.authenticator)
            .field("collect_metrics", &self.collect_metrics)
            .field("greeting", &self.greeting)
            .field("logger", &self.logger)
            .field("metrics", &self.collect_metrics)
            .field("passive_ports", &self.passive_ports)
            .field("passive_host", &self.passive_host)
            .field("ftps_client_auth", &self.ftps_client_auth)
            .field("ftps_mode", &self.ftps_mode)
            .field("ftps_required_control_chan", &self.ftps_required_control_chan)
            .field("ftps_required_data_chan", &self.ftps_required_data_chan)
            .field("ftps_tls_flags", &self.ftps_tls_flags)
            .field("ftps_trust_store", &self.ftps_trust_store)
            .field("idle_session_timeout", &self.idle_session_timeout)
            .field("proxy_protocol_mode", &self.proxy_protocol_mode)
            .field("proxy_protocol_switchboard", &self.proxy_protocol_switchboard)
            .finish()
    }
}

impl<Storage, User> Server<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`] generator and an [`AnonymousAuthenticator`]
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`AnonymousAuthenticator`]: ../auth/struct.AnonymousAuthenticator.html
    pub fn new(sbe_generator: Box<dyn (Fn() -> Storage) + Send + Sync>) -> Self
    where
        AnonymousAuthenticator: Authenticator<User>,
    {
        Self::with_authenticator(sbe_generator, Arc::new(AnonymousAuthenticator {}))
    }

    /// Construct a new [`Server`] with the given [`StorageBackend`] and [`Authenticator`]. The other parameters will be set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn with_authenticator(s: Box<dyn (Fn() -> Storage) + Send + Sync>, authenticator: Arc<dyn Authenticator<User> + Send + Sync>) -> Self {
        Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator,
            passive_ports: options::DEFAULT_PASSIVE_PORTS,
            passive_host: options::DEFAULT_PASSIVE_HOST,
            ftps_mode: FtpsConfig::Off,
            collect_metrics: false,
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
            proxy_protocol_mode: ProxyMode::Off,
            proxy_protocol_switchboard: Option::None,
            logger: slog::Logger::root(slog_stdlog::StdLog {}.fuse(), slog::o!()),
            ftps_required_control_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_required_data_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_tls_flags: TlsFlags::default(),
            ftps_client_auth: FtpsClientAuth::default(),
            ftps_trust_store: options::DEFAULT_FTPS_TRUST_STORE.into(),
            sitemd5: options::DEFAULT_SITEMD5,
        }
    }

    /// Set the [`Authenticator`] that will be used for authentication.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::{auth, auth::AnonymousAuthenticator, Server};
    /// use unftp_sbe_fs::ServerExt;
    /// use std::sync::Arc;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp")
    ///                  .authenticator(Arc::new(auth::AnonymousAuthenticator{}));
    /// ```
    ///
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn authenticator(mut self, authenticator: Arc<dyn Authenticator<User> + Send + Sync>) -> Self {
        self.authenticator = authenticator;
        self
    }

    /// Enables FTPS by configuring the path to the certificates file and the private key file. Both
    /// should be in PEM format.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .ftps("/srv/unftp/server.certs", "/srv/unftp/server.key");
    /// ```
    pub fn ftps<P: Into<PathBuf>>(mut self, certs_file: P, key_file: P) -> Self {
        self.ftps_mode = FtpsConfig::Building {
            certs_file: certs_file.into(),
            key_file: key_file.into(),
        };
        self
    }

    /// Allows switching on Mutual TLS. For this to work the trust anchors also needs to be set using
    /// the [ftps_trust_store](crate::Server::ftps_trust_store) method.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    /// use libunftp::options::FtpsClientAuth;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .ftps("/srv/unftp/server.certs", "/srv/unftp/server.key")
    ///              .ftps_client_auth(FtpsClientAuth::Require)
    ///              .ftps_trust_store("/srv/unftp/trusted.pem");
    /// ```

    pub fn ftps_client_auth<C>(mut self, auth: C) -> Self
    where
        C: Into<FtpsClientAuth>,
    {
        self.ftps_client_auth = auth.into();
        self
    }

    /// Configures whether client connections may use plaintext mode or not.
    pub fn ftps_required<R>(mut self, for_control_chan: R, for_data_chan: R) -> Self
    where
        R: Into<FtpsRequired>,
    {
        self.ftps_required_control_chan = for_control_chan.into();
        self.ftps_required_data_chan = for_data_chan.into();
        self
    }

    /// Sets the certificates to use when verifying client certificates in Mutual TLS mode. This
    /// should point to certificates in a PEM formatted file. For this to have any effect MTLS needs
    /// to be switched on via the [ftps_client_auth](crate::Server::ftps_client_auth) method.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .ftps("/srv/unftp/server.certs", "/srv/unftp/server.key")
    ///              .ftps_client_auth(true)
    ///              .ftps_trust_store("/srv/unftp/trusted.pem");
    /// ```
    pub fn ftps_trust_store<P>(mut self, trust: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.ftps_trust_store = trust.into();
        self
    }

    /// Switches TLS features on or off.
    ///
    /// # Example
    ///
    /// This example enables only TLS v1.3 and allows TLS session resumption with tickets.
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    /// use libunftp::options::TlsFlags;
    ///
    /// let mut server = Server::with_fs("/tmp")
    ///                  .greeting("Welcome to my FTP Server")
    ///                  .ftps("/srv/unftp/server.certs", "/srv/unftp/server.key")
    ///                  .ftps_tls_flags(TlsFlags::V1_3 | TlsFlags::RESUMPTION_TICKETS);
    /// ```
    pub fn ftps_tls_flags(mut self, flags: TlsFlags) -> Self {
        self.ftps_tls_flags = flags;
        self
    }

    /// Set the greeting that will be sent to the client after connecting.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp").greeting("Welcome to my FTP Server");
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_fs("/tmp");
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
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp").idle_session_timeout(600);
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_fs("/tmp");
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
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp").metrics();
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_fs("/tmp");
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
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host([127,0,0,1]);
    /// ```
    /// Or the same but more explicitly:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    /// use std::net::Ipv4Addr;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host(options::PassiveHost::Ip(Ipv4Addr::new(127, 0, 0, 1)));
    /// ```
    ///
    /// To determine the passive IP from the incoming control connection:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host(options::PassiveHost::FromConnection);
    /// ```
    ///
    /// Get the IP by resolving a DNS name:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
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
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let server = Server::with_fs("/tmp")
    ///              .passive_ports(49152..65535);
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_fs("/tmp");
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
    /// (<https://www.haproxy.org/download/1.8/doc/proxy-protocol.txt>).
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
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp").proxy_protocol_mode(2121);
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
    /// use unftp_sbe_fs::ServerExt;
    /// use tokio::runtime::Runtime;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let server = Server::with_fs("/srv/ftp");
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
    pub async fn listen<T: Into<String> + Debug>(mut self, bind_address: T) -> std::result::Result<(), ServerError> {
        self.ftps_mode = match self.ftps_mode {
            FtpsConfig::Off => FtpsConfig::Off,
            FtpsConfig::Building { certs_file, key_file } => FtpsConfig::On {
                tls_config: tls::new_config(certs_file, key_file, self.ftps_tls_flags, self.ftps_client_auth, self.ftps_trust_store.clone())?,
            },
            FtpsConfig::On { tls_config } => FtpsConfig::On { tls_config },
        };
        match self.proxy_protocol_mode {
            ProxyMode::On { external_control_port } => self.listen_proxy_protocol_mode(bind_address, external_control_port).await,
            ProxyMode::Off => self.listen_normal_mode(bind_address).await,
        }
    }

    #[tracing_attributes::instrument]
    async fn listen_normal_mode<T: Into<String> + Debug>(self, bind_address: T) -> std::result::Result<(), ServerError> {
        let addr: std::net::SocketAddr = bind_address.into().parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        loop {
            match listener.accept().await {
                Ok((tcp_stream, socket_addr)) => {
                    slog::info!(self.logger, "Incoming control connection from {:?}", socket_addr);
                    let params: controlchan::LoopConfig<Storage, User> = (&self).into();
                    let result = controlchan::spawn_loop::<Storage, User>(params, tcp_stream, None, None).await;
                    if let Err(err) = result {
                        slog::error!(
                            self.logger,
                            "Could not spawn control channel loop for connection from {:?}: {:?}",
                            socket_addr,
                            err
                        )
                    }
                }
                Err(err) => {
                    slog::error!(self.logger, "Error accepting incoming control connection {:?}", err);
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn listen_proxy_protocol_mode<T: Into<String> + Debug>(
        mut self,
        bind_address: T,
        external_control_port: u16,
    ) -> std::result::Result<(), ServerError> {
        let addr: std::net::SocketAddr = bind_address.into().parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;

        // this callback is used by all sessions, basically only to
        // request for a passive listening port.
        let (proxyloop_msg_tx, mut proxyloop_msg_rx): (ProxyLoopSender<Storage, User>, ProxyLoopReceiver<Storage, User>) = channel(1);

        loop {
            // The 'proxy loop' handles two kinds of events:
            // - incoming tcp connections originating from the proxy
            // - channel messages originating from PASV, to handle the passive listening port

            tokio::select! {

                Ok((tcp_stream, _socket_addr)) = listener.accept() => {
                    let socket_addr = tcp_stream.peer_addr();
                    let mut tcp_stream = tcp_stream;

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
                    let destination_port = connection.destination.port();
                    if destination_port == external_control_port {
                        let source = connection.source;
                        slog::info!(self.logger, "Connection from {:?} is a control connection", source);
                        let params: controlchan::LoopConfig<Storage,User> = (&self).into();
                        let result = controlchan::spawn_loop::<Storage,User>(params, tcp_stream, Some(source), Some(proxyloop_msg_tx.clone())).await;
                        if result.is_err() {
                            slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", result.err().unwrap())
                        }
                    } else {
                        // handle incoming data connections
                        slog::info!(self.logger, "Connection from {:?} is a data connection: {:?}, {}", socket_addr, self.passive_ports, destination_port);
                        if !self.passive_ports.contains(&destination_port) {
                            slog::warn!(self.logger, "Incoming proxy connection going to unconfigured port! This port is not configured as a passive listening port: port {} not in passive port range {:?}", destination_port, self.passive_ports);
                            tcp_stream.shutdown().await.unwrap();
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
    async fn dispatch_data_connection(&mut self, mut tcp_stream: tokio::net::TcpStream, connection: ConnectionTuple) {
        if let Some(switchboard) = &mut self.proxy_protocol_switchboard {
            match switchboard.get_session_by_incoming_data_connection(&connection).await {
                Some(session) => {
                    spawn_processing(self.logger.clone(), session, tcp_stream).await;
                    switchboard.unregister(&connection);
                }
                None => {
                    slog::warn!(self.logger, "Unexpected connection ({:?})", connection);
                    tcp_stream.shutdown().await.unwrap();
                    return;
                }
            }
        }
    }

    #[tracing_attributes::instrument]
    async fn select_and_register_passive_port(&mut self, session_arc: SharedSession<Storage, User>) {
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
        if let Some(destination) = session.destination {
            let destination_ip = match destination.ip() {
                IpAddr::V4(ip) => ip,
                IpAddr::V6(_) => panic!("Won't happen since PASV only does IP V4."),
            };

            let reply: Reply = super::controlchan::commands::make_pasv_reply(self.passive_host.clone(), &destination_ip, reserved_port).await;

            let tx_some = session.control_msg_tx.clone();
            if let Some(tx) = tx_some {
                let mut tx = tx.clone();
                tx.send(ControlChanMsg::CommandChannelReply(reply)).await.unwrap();
            }
        }
    }

    /// Enables SITE MD5
    ///
    /// Warning:
    /// Depending on the storage backend, SITE MD5 may use relatively much memory and generate high CPU usage.
    /// This opens a Denial of Service vulnerability that could be exploited by malicious users, by means of flooding the server with SITE MD5 commands.
    /// As such this feature is probably best user configured and at least disabled for anonymous users by default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp").sitemd5(SiteMd5::None);
    /// ```

    pub fn sitemd5<H: Into<SiteMd5>>(mut self, sitemd5_option: H) -> Self {
        self.sitemd5 = sitemd5_option.into();
        self
    }
}

impl<Storage, User> From<&Server<Storage, User>> for controlchan::LoopConfig<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    fn from(server: &Server<Storage, User>) -> Self {
        controlchan::LoopConfig {
            authenticator: server.authenticator.clone(),
            storage: (server.storage)(),
            ftps_config: server.ftps_mode.clone(),
            collect_metrics: server.collect_metrics,
            greeting: server.greeting,
            idle_session_timeout: server.idle_session_timeout,
            passive_ports: server.passive_ports.clone(),
            passive_host: server.passive_host.clone(),
            logger: server.logger.new(slog::o!()),
            ftps_required_control_chan: server.ftps_required_control_chan,
            ftps_required_data_chan: server.ftps_required_data_chan,
            sitemd5: server.sitemd5,
        }
    }
}
