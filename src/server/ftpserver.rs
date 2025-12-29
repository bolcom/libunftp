mod chosen;
pub mod error;
mod listen;
mod listen_prebound;
pub mod options;
mod mode;

use super::{
    controlchan,
    failed_logins::FailedLoginsCache,
    ftpserver::{error::ServerError, error::ShutdownError, options::FtpsRequired, options::SiteMd5},
    shutdown,
    tls::FtpsConfig,
};
use crate::server::switchboard::Switchboard;
use crate::{
    auth::{Authenticator, DefaultUser, DefaultUserDetailProvider, UserDetail, UserDetailProvider, anonymous::AnonymousAuthenticator},
    notification::{DataListener, PresenceListener, nop::NopListener},
    options::ActivePassiveMode,
    options::{FailedLoginsPolicy, FtpsClientAuth, TlsFlags},
    server::shutdown::Notifier,
    server::tls,
    storage::{Metadata, StorageBackend},
};
use options::{DEFAULT_GREETING, DEFAULT_IDLE_SESSION_TIMEOUT_SECS, PassiveHost};
#[cfg(feature = "experimental")]
use rustls::ServerConfig;
use slog::*;
use std::{ffi::OsString, fmt::Debug, future::Future, net::SocketAddr, ops::RangeInclusive, path::PathBuf, pin::Pin, sync::Arc, time::Duration};

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
/// rt.spawn(async {
///     let server = Server::with_fs("/srv/ftp").build().unwrap();
///     server.listen("127.0.0.1:2121").await.unwrap()
/// });
/// ```
///
/// [`Authenticator`]: crate::auth::Authenticator
/// [`StorageBackend`]: storage/trait.StorageBackend.html
pub struct Server<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    storage: Arc<dyn (Fn() -> Storage) + Send + Sync>,
    greeting: &'static str,
    authenticator: Arc<dyn Authenticator>,
    user_detail_provider: Arc<dyn UserDetailProvider<User = User> + Send + Sync>,
    data_listener: Arc<dyn DataListener>,
    presence_listener: Arc<dyn PresenceListener>,
    passive_ports: RangeInclusive<u16>,
    passive_host: PassiveHost,
    collect_metrics: bool,
    ftps_mode: FtpsConfig,
    ftps_required_control_chan: FtpsRequired,
    ftps_required_data_chan: FtpsRequired,
    idle_session_timeout: std::time::Duration,
    proxy_protocol_mode: ListenerMode,
    logger: slog::Logger,
    site_md5: SiteMd5,
    shutdown: Pin<Box<dyn Future<Output = options::Shutdown> + Send + Sync>>,
    failed_logins_policy: Option<FailedLoginsPolicy>,
    active_passive_mode: ActivePassiveMode,
    connection_helper: Option<OsString>,
    connection_helper_args: Vec<OsString>,
    binder: Arc<std::sync::Mutex<Option<Box<dyn crate::options::Binder>>>>,
}

/// Used to create [`Server`]s.
pub struct ServerBuilder<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    storage: Arc<dyn (Fn() -> Storage) + Send + Sync>,
    greeting: &'static str,
    authenticator: Arc<dyn Authenticator>,
    user_detail_provider: Arc<dyn UserDetailProvider<User = User> + Send + Sync>,
    data_listener: Arc<dyn DataListener>,
    presence_listener: Arc<dyn PresenceListener>,
    passive_ports: RangeInclusive<u16>,
    passive_host: PassiveHost,
    collect_metrics: bool,
    ftps_mode: FtpsConfig,
    ftps_required_control_chan: FtpsRequired,
    ftps_required_data_chan: FtpsRequired,
    ftps_tls_flags: TlsFlags,
    ftps_client_auth: FtpsClientAuth,
    ftps_trust_store: PathBuf,
    idle_session_timeout: std::time::Duration,
    listener_mode: ListenerMode,
    logger: slog::Logger,
    site_md5: SiteMd5,
    shutdown: Pin<Box<dyn Future<Output = options::Shutdown> + Send + Sync>>,
    failed_logins_policy: Option<FailedLoginsPolicy>,
    active_passive_mode: ActivePassiveMode,
    connection_helper: Option<OsString>,
    connection_helper_args: Vec<OsString>,
    binder: Option<Box<dyn crate::options::Binder>>,
}

impl<Storage> ServerBuilder<Storage, DefaultUser>
where
    Storage: StorageBackend<DefaultUser> + 'static,
    Storage::Metadata: Metadata,
{
    /// Construct a new [`ServerBuilder`] with the given [`StorageBackend`] generator and an [`AnonymousAuthenticator`]
    ///
    /// [`ServerBuilder`]: struct.ServerBuilder.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`AnonymousAuthenticator`]: ../auth/struct.AnonymousAuthenticator.html
    pub fn new(sbe_generator: Box<dyn Fn() -> Storage + Send + Sync>) -> Self {
        Self::with_authenticator(sbe_generator, Arc::new(AnonymousAuthenticator {}))
    }

    /// Construct a new [`ServerBuilder`] with the given [`StorageBackend`] generator and [`Authenticator`]. The other parameters will be set to defaults.
    ///
    /// [`ServerBuilder`]: struct.ServerBuilder.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn with_authenticator(sbe_generator: Box<dyn (Fn() -> Storage) + Send + Sync>, authenticator: Arc<dyn Authenticator + Send + Sync>) -> Self {
        let passive_ports = options::DEFAULT_PASSIVE_PORTS;
        ServerBuilder {
            storage: Arc::from(sbe_generator),
            greeting: DEFAULT_GREETING,
            authenticator,
            user_detail_provider: Arc::new(DefaultUserDetailProvider {}),
            data_listener: Arc::new(NopListener {}),
            presence_listener: Arc::new(NopListener {}),
            passive_ports,
            passive_host: options::DEFAULT_PASSIVE_HOST,
            ftps_mode: FtpsConfig::Off,
            collect_metrics: false,
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
            listener_mode: ListenerMode::Legacy,
            logger: slog::Logger::root(slog_stdlog::StdLog {}.fuse(), slog::o!()),
            ftps_required_control_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_required_data_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_tls_flags: TlsFlags::default(),
            ftps_client_auth: FtpsClientAuth::default(),
            ftps_trust_store: options::DEFAULT_FTPS_TRUST_STORE.into(),
            site_md5: SiteMd5::default(),
            shutdown: Box::pin(futures_util::future::pending()),
            failed_logins_policy: None,
            active_passive_mode: ActivePassiveMode::default(),
            connection_helper: None,
            connection_helper_args: Vec::new(),
            binder: None,
        }
    }

    /// Set the [`UserDetailProvider`] that will be used to convert authenticated principals
    /// into full user details.
    ///
    /// This method allows you to specify a provider that converts a [`Principal`] (returned by
    /// the [`Authenticator`]) into a full [`UserDetail`] implementation with additional user
    /// information such as home directory and account settings.
    ///
    /// This method changes the `User` type parameter of the `ServerBuilder` to match the
    /// `User` type provided by the `UserDetailProvider`. This allows the builder to work with
    /// the specific user type throughout the server configuration.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::{auth::DefaultUserDetailProvider, Server};
    /// use unftp_sbe_fs::ServerExt;
    /// use std::sync::Arc;
    ///
    /// let server = Server::with_fs("/tmp")
    ///     .user_detail_provider(Arc::new(DefaultUserDetailProvider))
    ///     .build();
    /// ```
    ///
    /// [`UserDetailProvider`]: ../auth/trait.UserDetailProvider.html
    /// [`Principal`]: ../auth/struct.Principal.html
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    /// [`UserDetail`]: ../auth/trait.UserDetail.html
    pub fn user_detail_provider<U, P>(self, provider: Arc<P>) -> ServerBuilder<Storage, U>
    where
        U: UserDetail + 'static,
        P: UserDetailProvider<User = U> + Send + Sync + 'static,
        Storage: StorageBackend<U>,
    {
        ServerBuilder {
            storage: self.storage,
            greeting: self.greeting,
            authenticator: self.authenticator,
            user_detail_provider: provider,
            data_listener: self.data_listener,
            presence_listener: self.presence_listener,
            passive_ports: self.passive_ports,
            passive_host: self.passive_host,
            collect_metrics: self.collect_metrics,
            ftps_mode: self.ftps_mode,
            ftps_required_control_chan: self.ftps_required_control_chan,
            ftps_required_data_chan: self.ftps_required_data_chan,
            ftps_tls_flags: self.ftps_tls_flags,
            ftps_client_auth: self.ftps_client_auth,
            ftps_trust_store: self.ftps_trust_store,
            idle_session_timeout: self.idle_session_timeout,
            listener_mode: self.listener_mode,
            logger: self.logger,
            site_md5: self.site_md5,
            shutdown: self.shutdown,
            failed_logins_policy: self.failed_logins_policy,
            active_passive_mode: self.active_passive_mode,
            connection_helper: self.connection_helper,
            connection_helper_args: self.connection_helper_args,
            binder: self.binder,
        }
    }
}

impl<Storage, User> ServerBuilder<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    /// Construct a new [`ServerBuilder`] with the given [`StorageBackend`] generator and [`UserDetailProvider`].
    /// An [`AnonymousAuthenticator`] will be used as the default authenticator. The other parameters will be set to defaults.
    ///
    /// This method allows you to specify a custom user type by providing a `UserDetailProvider` that converts
    /// authenticated principals to your custom user type.
    ///
    /// [`ServerBuilder`]: struct.ServerBuilder.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    /// [`UserDetailProvider`]: ../auth/trait.UserDetailProvider.html
    /// [`AnonymousAuthenticator`]: ../auth/struct.AnonymousAuthenticator.html
    pub fn with_user_detail_provider<U>(
        sbe_generator: Box<dyn Fn() -> Storage + Send + Sync>,
        provider: Arc<dyn UserDetailProvider<User = U> + Send + Sync>,
    ) -> ServerBuilder<Storage, U>
    where
        U: UserDetail + 'static,
        Storage: StorageBackend<U> + 'static,
        <Storage as StorageBackend<U>>::Metadata: Metadata,
    {
        let passive_ports = options::DEFAULT_PASSIVE_PORTS;
        ServerBuilder {
            storage: Arc::from(sbe_generator),
            greeting: DEFAULT_GREETING,
            authenticator: Arc::new(AnonymousAuthenticator {}),
            user_detail_provider: provider,
            data_listener: Arc::new(NopListener {}),
            presence_listener: Arc::new(NopListener {}),
            passive_ports,
            passive_host: options::DEFAULT_PASSIVE_HOST,
            ftps_mode: FtpsConfig::Off,
            collect_metrics: false,
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
            listener_mode: ListenerMode::Legacy,
            logger: slog::Logger::root(slog_stdlog::StdLog {}.fuse(), slog::o!()),
            ftps_required_control_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_required_data_chan: options::DEFAULT_FTPS_REQUIRE,
            ftps_tls_flags: TlsFlags::default(),
            ftps_client_auth: FtpsClientAuth::default(),
            ftps_trust_store: options::DEFAULT_FTPS_TRUST_STORE.into(),
            site_md5: SiteMd5::default(),
            shutdown: Box::pin(futures_util::future::pending()),
            failed_logins_policy: None,
            active_passive_mode: ActivePassiveMode::default(),
            connection_helper: None,
            connection_helper_args: Vec::new(),
            binder: None,
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
    /// let server = Server::with_fs("/tmp")
    ///                  .authenticator(Arc::new(auth::AnonymousAuthenticator{}))
    ///                  .build();
    /// ```
    ///
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn authenticator(mut self, authenticator: Arc<dyn Authenticator + Send + Sync>) -> Self {
        self.authenticator = authenticator;
        self
    }

    /// Enables one or both of Active/Passive mode. In active mode the server connects to the client's
    /// data port and in passive mode the client connects the the server's data port.
    ///
    /// Active mode is an older mode and considered less secure and is therefore disabled by default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::options::ActivePassiveMode;
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .active_passive_mode(ActivePassiveMode::ActiveAndPassive)
    ///              .build();
    /// ```
    pub fn active_passive_mode<M: Into<ActivePassiveMode>>(mut self, mode: M) -> Self {
        self.active_passive_mode = mode.into();
        self
    }

    /// Finalize the options and build a [`Server`].
    pub fn build(self) -> std::result::Result<Server<Storage, User>, ServerError> {
        let ftps_mode = match self.ftps_mode {
            FtpsConfig::Off => FtpsConfig::Off,
            FtpsConfig::Building { certs_file, key_file } => FtpsConfig::On {
                tls_config: tls::new_config(certs_file, key_file, self.ftps_tls_flags, self.ftps_client_auth, self.ftps_trust_store.clone())?,
            },
            FtpsConfig::On { tls_config } => FtpsConfig::On { tls_config },
        };
        let binder = Arc::new(std::sync::Mutex::new(self.binder));
        Ok(Server {
            storage: self.storage,
            greeting: self.greeting,
            authenticator: self.authenticator,
            user_detail_provider: self.user_detail_provider,
            data_listener: self.data_listener,
            presence_listener: self.presence_listener,
            passive_ports: self.passive_ports,
            passive_host: self.passive_host,
            collect_metrics: self.collect_metrics,
            ftps_mode,
            ftps_required_control_chan: self.ftps_required_control_chan,
            ftps_required_data_chan: self.ftps_required_data_chan,
            idle_session_timeout: self.idle_session_timeout,
            proxy_protocol_mode: self.listener_mode,
            logger: self.logger,
            site_md5: self.site_md5,
            shutdown: self.shutdown,
            failed_logins_policy: self.failed_logins_policy,
            active_passive_mode: self.active_passive_mode,
            connection_helper: self.connection_helper,
            connection_helper_args: self.connection_helper_args,
            binder,
        })
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

    /// Enables FTPS by configuring the raw FtpsConfig. Needs the `experimental` feature to
    /// be switched on.
    ///
    /// # Example
    ///
    /// ```rust
    /// # let config = Default::default();
    /// use rustls::ServerConfig;
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .ftps_manual(config);
    /// ```
    #[cfg(feature = "experimental")]
    pub fn ftps_manual<P: Into<PathBuf>>(mut self, config: Arc<ServerConfig>) -> Self {
        self.ftps_mode = FtpsConfig::On { tls_config: config };
        self
    }

    /// Allows switching on Mutual TLS. For this to work the trust anchors also needs to be set using
    /// the [ftps_trust_store](crate::ServerBuilder::ftps_trust_store) method.
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
    /// to be switched on via the [ftps_client_auth](crate::ServerBuilder::ftps_client_auth) method.
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
    /// let server = Server::with_fs("/tmp")
    ///     .greeting("Welcome to my FTP Server")
    ///     .build();
    //
    /// // Or instead if you prefer:
    /// let mut server = Server::with_fs("/tmp");
    /// server.greeting("Welcome to my FTP Server").build();
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
    /// let mut builder = Server::with_fs("/tmp").metrics();
    ///
    /// // Or instead if you prefer:
    /// let mut builder = Server::with_fs("/tmp");
    /// builder.metrics();
    /// ```
    pub fn metrics(mut self) -> Self {
        self.collect_metrics = true;
        self
    }

    /// Sets an [`DataListener`](crate::notification::DataListener) that will
    /// be notified of data changes that happen in a user's session.
    pub fn notify_data(mut self, listener: impl DataListener + 'static) -> Self {
        self.data_listener = Arc::new(listener);
        self
    }

    /// Sets an [`PresenceListener`](crate::notification::PresenceListener) that will
    /// be notified of user logins and logouts
    pub fn notify_presence(mut self, listener: impl PresenceListener + 'static) -> Self {
        self.presence_listener = Arc::new(listener);
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
    ///              .passive_host([127,0,0,1])
    ///              .build();
    /// ```
    /// Or the same but more explicitly:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    /// use std::net::Ipv4Addr;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host(options::PassiveHost::Ip(Ipv4Addr::new(127, 0, 0, 1)))
    ///              .build();
    /// ```
    ///
    /// To determine the passive IP from the incoming control connection:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host(options::PassiveHost::FromConnection)
    ///              .build();
    /// ```
    ///
    /// Get the IP by resolving a DNS name:
    ///
    /// ```rust
    /// use libunftp::{Server,options};
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///              .passive_host("ftp.myserver.org")
    ///              .build();
    /// ```
    pub fn passive_host<H: Into<PassiveHost>>(mut self, host_option: H) -> Self {
        self.passive_host = host_option.into();
        self
    }

    /// Set a callback for binding sockets
    ///
    /// If present, this helper will be used for binding sockets instead of the standard routines
    /// in std or tokio.  It can be useful in capability mode.
    pub fn binder<B: crate::options::Binder + 'static>(mut self, b: B) -> Self {
        self.binder = Some(Box::new(b));
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
    /// let builder = Server::with_fs("/tmp")
    ///              .passive_ports(49152..=65535);
    ///
    /// // Or instead if you prefer:
    /// let mut builder = Server::with_fs("/tmp");
    /// builder.passive_ports(49152..=65535);
    /// ```
    pub fn passive_ports(mut self, range: RangeInclusive<u16>) -> Self {
        self.passive_ports = range;
        self
    }

    /// Enables pooled listener mode.
    ///
    /// In Pooled mode, all passive ports are continuously listening
    /// allows very high connection concurrency.
    ///
    /// Where in the legacy listener mode, each passive port requested
    /// via PASV leads to a port bind, in Pooled mode, all ports
    /// are already bound and listening, and a PASV simply assigns
    /// one to the session.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_fs("/tmp")
    ///     .pooled_listener_mode(2121)
    ///     .build();
    /// ```
    pub fn pooled_listener_mode(mut self) -> Self {
        self.listener_mode = ListenerMode::Pooled;
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
    /// let mut server = Server::with_fs("/tmp")
    ///     .proxy_protocol_mode(2121)
    ///     .build();
    /// ```
    #[cfg(feature = "proxy_protocol")]
    pub fn proxy_protocol_mode(mut self, external_control_port: u16) -> Self {
        self.listener_mode = external_control_port.into();
        self
    }

    /// Allows telling libunftp when and how to shutdown gracefully.
    ///
    /// The passed argument is a future that resolves when libunftp should shut down. The future
    /// should return a [options::Shutdown](options::Shutdown) instance.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/tmp")
    ///     .shutdown_indicator(async {
    ///         // Shut the server down after 10 seconds.
    ///         tokio::time::sleep(Duration::from_secs(10)).await;
    ///         libunftp::options::Shutdown::new()
    ///              .grace_period(Duration::from_secs(5)) // Allow 5 seconds to shutdown gracefully
    ///     }).build();
    /// ```
    pub fn shutdown_indicator<I>(mut self, indicator: I) -> Self
    where
        I: Future<Output = options::Shutdown> + Send + Sync + 'static,
    {
        self.shutdown = Box::pin(indicator);
        self
    }

    /// Enables the FTP command 'SITE MD5'.
    ///
    /// _Warning:_ Depending on the storage backend, SITE MD5 may use relatively much memory and
    /// generate high CPU usage. This opens a Denial of Service vulnerability that could be exploited
    /// by malicious users, by means of flooding the server with SITE MD5 commands. As such this
    /// feature is probably best user configured and at least disabled for anonymous users by default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use libunftp::options::SiteMd5;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // Use it in a builder-like pattern:
    /// let server = Server::with_fs("/tmp")
    ///     .sitemd5(SiteMd5::None)
    ///     .build();
    /// ```
    pub fn sitemd5<M: Into<SiteMd5>>(mut self, sitemd5_option: M) -> Self {
        self.site_md5 = sitemd5_option.into();
        self
    }

    /// Assign a connection helper to the server.
    ///
    /// Rather than listening for and servicing connections in the same binary, this option allows
    /// accepted connections to be serviced by a different program.  After accepting a connection,
    /// the Server will execute the provided helper process.  Any provided arguments will be passed
    /// to the helper process.  After those arguments, the Server will pass an integer, which is
    /// the file descriptor number of the connected socket.
    ///
    /// # Arguments
    ///
    /// - `path` - Path to the helper executable
    /// - `args` - Optional arguments to pass to the helper executable.
    #[cfg(unix)]
    pub fn connection_helper(mut self, path: OsString, args: Vec<OsString>) -> Self {
        self.connection_helper = Some(path);
        self.connection_helper_args = args;
        self
    }

    /// Enables a password guessing protection policy
    ///
    /// Policy used to temporarily block an account, source IP or the
    /// combination of both, after a certain number of failed login
    /// attempts for a certain time.
    ///
    /// There are different policies to choose from. Such as to lock
    /// based on the combination of source IP + username or only
    /// username or IP. For example, if you choose IP based blocking,
    /// multiple successive failed login attempts will block any login
    /// attempt from that IP for a defined period, including login
    /// attempts for other users.
    ///
    /// The default policy is to block on the combination of source IP
    /// and username. This policy affects only this specific
    /// IP+username combination, and does not block the user logging
    /// in from elsewhere.
    ///
    /// It is also possible to override the default 'Penalty', which
    /// defines how many failed login attempts before applying the
    /// policy, and after what time the block expires.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use libunftp::options::{FailedLoginsPolicy,FailedLoginsBlock};
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// // With default policy
    /// let server =
    /// Server::with_fs("/tmp")
    ///     .failed_logins_policy(FailedLoginsPolicy::default())
    ///     .build();
    ///
    /// // Or choose a specific policy like based on source IP and
    /// // longer block (maximum 3 attempts, 5 minutes, IP based
    /// // blocking)
    /// use std::time::Duration;
    /// let server = Server::with_fs("/tmp")
    ///     .failed_logins_policy(FailedLoginsPolicy::new(3, Duration::from_secs(300), FailedLoginsBlock::IP))
    ///     .build();
    /// ```
    pub fn failed_logins_policy(mut self, policy: FailedLoginsPolicy) -> Self {
        self.failed_logins_policy = Some(policy);
        self
    }
}

impl<Storage, User> Server<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    /// Runs the main FTP process asynchronously. Should be started in a async runtime context.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    /// use tokio::runtime::Runtime;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// rt.spawn(async {
    ///     let server = Server::with_fs("/srv/ftp").build().unwrap();
    ///     server.listen("127.0.0.1:2121").await
    /// });
    /// // ...
    /// drop(rt);
    /// ```
    ///
    #[tracing_attributes::instrument]
    pub async fn listen<T: Into<String> + Debug>(self, bind_address: T) -> std::result::Result<(), ServerError> {
        let logger = self.logger.clone();
        let bind_address: SocketAddr = bind_address.into().parse()?;
        let shutdown_notifier = Arc::new(shutdown::Notifier::new());

        let failed_logins = self.failed_logins_policy.as_ref().map(|policy| FailedLoginsCache::new(policy.clone()));

        let listen_future = match self.proxy_protocol_mode {
            #[cfg(feature = "proxy_protocol")]
            ListenerMode::ProxyProtocol { external_control_port } => Box::pin(
                listen_prebound::PreboundListener {
                    bind_address,
                    logger: self.logger.clone(),
                    external_control_port,
                    options: (&self).into(),
                    switchboard: Switchboard::new(self.logger.clone(), self.passive_ports.clone()),
                    shutdown_topic: shutdown_notifier.clone(),
                    failed_logins: failed_logins.clone(),
                }
                .listen_proxy_protocol(),
            )
                as Pin<Box<dyn Future<Output = std::result::Result<(), ServerError>> + Send>>,
            ListenerMode::Pooled => Box::pin(
                listen_prebound::PreboundListener {
                    bind_address,
                    logger: self.logger.clone(),
                    external_control_port: None,
                    options: (&self).into(),
                    switchboard: Switchboard::new(self.logger.clone(), self.passive_ports.clone()),
                    shutdown_topic: shutdown_notifier.clone(),
                    failed_logins: failed_logins.clone(),
                }
                .listen_pooled(),
            ) as Pin<Box<dyn Future<Output = std::result::Result<(), ServerError>> + Send>>,
            ListenerMode::Legacy => Box::pin(
                listen::Listener {
                    bind_address,
                    logger: self.logger.clone(),
                    options: (&self).into(),
                    shutdown_topic: shutdown_notifier.clone(),
                    failed_logins: failed_logins.clone(),
                    connection_helper: self.connection_helper.clone(),
                    connection_helper_args: self.connection_helper_args.clone(),
                }
                .listen(),
            ) as Pin<Box<dyn Future<Output = std::result::Result<(), ServerError>> + Send>>,
        };

        let sweeper_fut = if let Some(ref failed_logins) = failed_logins {
            Box::pin(failed_logins.sweeper(self.logger.clone(), shutdown_notifier.clone())) as Pin<Box<dyn futures_util::Future<Output = ()> + Send>>
        } else {
            Box::pin(futures_util::future::pending()) as Pin<Box<dyn futures_util::Future<Output = ()> + Send>>
        };
        tokio::select! {
            result = listen_future => result,
            _ = sweeper_fut => {
                Ok(())
            },
            opts = self.shutdown => {
                slog::debug!(logger, "Shutting down within {:?}", opts.grace_period);
                shutdown_notifier.notify().await;
                Self::shutdown_linger(logger, shutdown_notifier, opts.grace_period).await
            }
        }
    }

    /// Service a newly established connection as a control connection.
    ///
    /// Use this method instead of [`listen`](Server::listen) if you want to listen for and accept
    /// new connections yourself, instead of using libunftp to do it.
    pub async fn service(self, tcp_stream: tokio::net::TcpStream) -> std::result::Result<(), crate::server::ControlChanError> {
        let failed_logins = self.failed_logins_policy.as_ref().map(|policy| FailedLoginsCache::new(policy.clone()));
        let options: chosen::OptionsHolder<Storage, User> = (&self).into();
        let shutdown_notifier = Arc::new(shutdown::Notifier::new());
        let shutdown_listener = shutdown_notifier.subscribe().await;
        slog::debug!(self.logger, "Servicing control connection from");
        let result = controlchan::spawn_loop::<Storage, User>((&options).into(), tcp_stream, None, None, shutdown_listener, failed_logins.clone()).await;
        match result {
            Err(err) => {
                slog::error!(self.logger, "Could not spawn control channel loop: {:?}", err);
            }
            Ok(jh) => {
                if let Err(e) = jh.await {
                    slog::error!(self.logger, "Control loop failed to complete: {:?}", e);
                }
            }
        }
        Ok(())
    }

    // Waits for sub-components to shut down gracefully or aborts if the grace period expires
    async fn shutdown_linger(logger: slog::Logger, shutdown_notifier: Arc<Notifier>, grace_period: Duration) -> std::result::Result<(), ServerError> {
        let timeout = Box::pin(tokio::time::sleep(grace_period));
        tokio::select! {
            _ = shutdown_notifier.linger() => {
                slog::debug!(logger, "Graceful shutdown complete");
                Ok(())
            },
            _ = timeout => {
                Err(ShutdownError{ msg: "shutdown grace period expired".to_string()}.into())
            }
        }
        // TODO: Implement feature where we keep on listening for a while i.e. GracefulAcceptingConnections
    }
}

impl<Storage, User> From<&Server<Storage, User>> for chosen::OptionsHolder<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    fn from(server: &Server<Storage, User>) -> Self {
        chosen::OptionsHolder {
            authenticator: server.authenticator.clone(),
            user_detail_provider: server.user_detail_provider.clone(),
            storage: server.storage.clone(),
            ftps_config: server.ftps_mode.clone(),
            collect_metrics: server.collect_metrics,
            greeting: server.greeting,
            idle_session_timeout: server.idle_session_timeout,
            passive_ports: server.passive_ports.clone(),
            passive_host: server.passive_host.clone(),
            logger: server.logger.new(slog::o!()),
            ftps_required_control_chan: server.ftps_required_control_chan,
            ftps_required_data_chan: server.ftps_required_data_chan,
            site_md5: server.site_md5,
            data_listener: server.data_listener.clone(),
            presence_listener: server.presence_listener.clone(),
            active_passive_mode: server.active_passive_mode,
            binder: server.binder.clone(),
        }
    }
}

impl<Storage, User> Debug for ServerBuilder<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerBuilder")
            .field("authenticator", &self.authenticator)
            .field("user_detail_provider", &self.user_detail_provider)
            .field("collect_metrics", &self.collect_metrics)
            .field("active_passive_mode", &self.active_passive_mode)
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
            .field("proxy_protocol_mode", &self.listener_mode)
            .field("failed_logins_policy", &self.failed_logins_policy)
            .finish()
    }
}

impl<Storage, User> Debug for Server<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerBuilder")
            .field("authenticator", &self.authenticator)
            .field("user_detail_provider", &self.user_detail_provider)
            .field("collect_metrics", &self.collect_metrics)
            .field("active_passive_mode", &self.active_passive_mode)
            .field("greeting", &self.greeting)
            .field("logger", &self.logger)
            .field("metrics", &self.collect_metrics)
            .field("passive_ports", &self.passive_ports)
            .field("passive_host", &self.passive_host)
            .field("ftps_mode", &self.ftps_mode)
            .field("ftps_required_control_chan", &self.ftps_required_control_chan)
            .field("ftps_required_data_chan", &self.ftps_required_data_chan)
            .field("idle_session_timeout", &self.idle_session_timeout)
            .field("proxy_protocol_mode", &self.proxy_protocol_mode)
            .field("failed_logins_policy", &self.failed_logins_policy)
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::server) enum ListenerMode {
    Legacy,
    Pooled,
    ProxyProtocol { external_control_port: Option<u16> },
}

impl From<u16> for ListenerMode {
    fn from(port: u16) -> Self {
        ListenerMode::ProxyProtocol {
            external_control_port: Some(port),
        }
    }
}
