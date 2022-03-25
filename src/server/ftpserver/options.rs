//! Contains code pertaining to the setup options that can be given to the [`Server`](crate::Server)

use bitflags::bitflags;
use std::time::Duration;
use std::{
    fmt::Formatter,
    fmt::{self, Debug, Display},
    net::{IpAddr, Ipv4Addr},
    ops::Range,
};

// Once we're sure about the types of these I think its good to expose it to the API user so that
// he/she can see what our server defaults are.
pub(crate) const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
pub(crate) const DEFAULT_IDLE_SESSION_TIMEOUT_SECS: u64 = 600;
pub(crate) const DEFAULT_PASSIVE_HOST: PassiveHost = PassiveHost::FromConnection;
pub(crate) const DEFAULT_PASSIVE_PORTS: Range<u16> = 49152..65535;
pub(crate) const DEFAULT_FTPS_REQUIRE: FtpsRequired = FtpsRequired::None;
pub(crate) const DEFAULT_FTPS_TRUST_STORE: &str = "./trusted.pem";

/// The option to [Server.passive_host](crate::Server::passive_host). It allows the user to specify how the IP address
/// communicated in the _PASV_ response is determined.
#[derive(Debug, PartialEq, Clone)]
pub enum PassiveHost {
    /// Use the IP address of the control connection
    FromConnection,
    /// Advertise this specific IP address
    Ip(Ipv4Addr),
    /// Resolve this DNS name into an IPv4 address.
    Dns(String),
    // We also be nice to have:
    // - PerUser(Box<dyn (Fn(Box<dyn UserDetail>) -> Ipv4Addr) + Send + Sync>) or something like
    //   that to allow a per user decision
}

impl Eq for PassiveHost {}

impl Default for PassiveHost {
    fn default() -> Self {
        PassiveHost::FromConnection
    }
}

impl From<Ipv4Addr> for PassiveHost {
    fn from(ip: Ipv4Addr) -> Self {
        PassiveHost::Ip(ip)
    }
}

impl From<[u8; 4]> for PassiveHost {
    fn from(ip: [u8; 4]) -> Self {
        PassiveHost::Ip(ip.into())
    }
}

impl From<&str> for PassiveHost {
    fn from(dns_or_ip: &str) -> Self {
        match dns_or_ip.parse() {
            Ok(IpAddr::V4(ip)) => PassiveHost::Ip(ip),
            _ => PassiveHost::Dns(dns_or_ip.to_string()),
        }
    }
}

/// The option to [Server.ftps_required](crate::Server::ftps_required). It allows the user to specify whether clients are required
/// to upgrade a to secure TLS connection i.e. use FTPS.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FtpsRequired {
    /// All users, including anonymous must use FTPS
    All,
    /// All non-anonymous users requires FTPS.
    Accounts,
    /// FTPS not enforced.
    None, // would be nice to have a per-user setting also.
}

impl Eq for FtpsRequired {}

impl From<bool> for FtpsRequired {
    fn from(on: bool) -> Self {
        match on {
            true => FtpsRequired::All,
            false => FtpsRequired::None,
        }
    }
}

impl Display for FtpsRequired {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FtpsRequired::All => "All users, including anonymous, requires FTPS",
                FtpsRequired::Accounts => "All non-anonymous users requires FTPS",
                FtpsRequired::None => "FTPS not enforced",
            }
        )
    }
}

bitflags! {
    /// Used to configure TLS options employed for FTPS
    pub struct TlsFlags: u32 {
        /// Enables TLS version 1.2
        const V1_2               = 0b00000001;
        /// Enables TLS version 1.3
        const V1_3               = 0b00000010;
        /// Enables TLS session resumption via means of sever side session IDs.
        const RESUMPTION_SESS_ID = 0b00001000;
        /// Enables TLS session resumption via means tickets ([rfc5077](https://tools.ietf.org/html/rfc5077))
        const RESUMPTION_TICKETS = 0b00010000;
        /// Enables the latest safe TLS versions i.e. 1.2 and 1.3
        const LATEST_VERSIONS = Self::V1_2.bits | Self::V1_3.bits;
    }
}

impl Default for TlsFlags {
    fn default() -> TlsFlags {
        // Switch TLS 1.3 off by default since we still see a PUT bug with lftp when switching
        // session resumption on along with TLS 1.3.
        TlsFlags::V1_2 | TlsFlags::RESUMPTION_SESS_ID | TlsFlags::RESUMPTION_TICKETS
    }
}

/// The option to [Server.ftps_client_auth](crate::Server::ftps_client_auth). Tells if and how mutual TLS (client certificate
/// authentication) should be handled.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FtpsClientAuth {
    /// Mutual TLS is switched off and the server won't ask the client for a certificate in the TLS
    /// protocol. This is the default.
    Off,
    /// Mutual TLS is on and whilst the server will request a certificate it will still proceed
    /// without one. If a certificate is sent by the client it will be validated against the
    /// configured trust anchors (see [Server::ftps_trust_store](crate::Server::ftps_trust_store)).
    Request,
    /// Mutual TLS is on, the server will request a certificate and it won't proceed without a
    /// client certificate that validates against the configured trust anchors (see
    /// [Server::ftps_trust_store](crate::Server::ftps_trust_store)).
    Require,
}

impl Eq for FtpsClientAuth {}

impl Default for FtpsClientAuth {
    fn default() -> FtpsClientAuth {
        FtpsClientAuth::Off
    }
}

impl From<bool> for FtpsClientAuth {
    fn from(on: bool) -> Self {
        match on {
            true => FtpsClientAuth::Require,
            false => FtpsClientAuth::Off,
        }
    }
}

/// The options for [Server.sitemd5](crate::Server::sitemd5).
/// Allow MD5 either to be used by all, logged in users only or no one.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SiteMd5 {
    /// Enabled for all users, including anonymous
    All,
    /// Enabled for all non-anonymous users.
    Accounts,
    /// Disabled
    None, // would be nice to have a per-user setting also.
}

impl Default for SiteMd5 {
    fn default() -> SiteMd5 {
        SiteMd5::Accounts
    }
}

/// Tells how graceful shutdown should happen. An instance of this struct should be returned from
/// the future passed to [Server.shutdown_indicator](crate::Server::shutdown_indicator).
pub struct Shutdown {
    pub(crate) grace_period: Duration,
    //pub(crate) handle_new_connections: bool,
}

impl Shutdown {
    /// Creates a Shutdown instance with default values
    pub fn new() -> Self {
        Shutdown::default()
    }

    /// Defines how much time to allow for components to shut down before shutdown is forceful.
    pub fn grace_period(mut self, d: impl Into<Duration>) -> Self {
        self.grace_period = d.into();
        self
    }

    // /// Control channel connections will still be accepted for a while as connections
    // /// are drained. Clients connecting during this phase will receive an FTP error code.
    // pub fn handle_new_connections(mut self) -> Self {
    //     self.handle_new_connections = true;
    //     self
    // }
    //
    // /// Control channel connections will not be allowed during the shutdown phase.
    // pub fn block_new_connections(mut self) -> Self {
    //     self.handle_new_connections = false;
    //     self
    // }
}

impl Default for Shutdown {
    fn default() -> Shutdown {
        Shutdown {
            grace_period: Duration::from_secs(10),
            //handle_new_connections: false,
        }
    }
}

#[derive(Debug, Clone)]
/// Variants for failed logins protection policy
pub enum FailedLoginsPolicy {
    /// User plus source IP address locking
    SourceUserLock(FailedLoginsPenalty),
    /// Source IP locking
    SourceLock(FailedLoginsPenalty),
    /// Lock the user
    UserLock(FailedLoginsPenalty),
}

impl FailedLoginsPenalty {
    /// Create a new FailedLoginsPenalty instance
    pub fn new(max_attempts: u32, expires_after: Duration) -> FailedLoginsPenalty {
        FailedLoginsPenalty { max_attempts, expires_after }
    }
}

#[derive(Debug, Clone)]
/// Describes the exact penalty
pub struct FailedLoginsPenalty {
    /// The maximum number of consecutive failed login attempts before the account gets locked
    pub(crate) max_attempts: u32,
    /// The expiration time since the last failed login attempt that the account gets unlocked
    pub(crate) expires_after: Duration,
}

impl Default for FailedLoginsPenalty {
    fn default() -> FailedLoginsPenalty {
        FailedLoginsPenalty {
            max_attempts: 3,
            expires_after: Duration::from_secs(120),
        }
    }
}

impl Default for FailedLoginsPolicy {
    fn default() -> FailedLoginsPolicy {
        FailedLoginsPolicy::SourceUserLock(FailedLoginsPenalty::default())
    }
}
