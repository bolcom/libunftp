//! Contains code pertaining to the setup options that can be given to the [`Server`](crate::Server)

use bitflags::bitflags;
use std::fmt::Formatter;
use std::ops::Range;
use std::{
    fmt::{self, Debug, Display},
    net::{IpAddr, Ipv4Addr},
};

// Once we're sure about the types of these I think its good to expose it to the API user so that
// he/she can see what our server defaults are.
pub(crate) const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
pub(crate) const DEFAULT_IDLE_SESSION_TIMEOUT_SECS: u64 = 600;
pub(crate) const DEFAULT_PASSIVE_HOST: PassiveHost = PassiveHost::FromConnection;
pub(crate) const DEFAULT_PASSIVE_PORTS: Range<u16> = 49152..65535;
pub(crate) const DEFAULT_FTPS_REQUIRE: FtpsRequired = FtpsRequired::None;
pub(crate) const DEFAULT_FTPS_TRUST_STORE: &str = "./trusted.pem";
pub(crate) const DEFAULT_SITEMD5: SiteMd5 = SiteMd5::Accounts;

/// The option to `Server.passive_host`. It allows the user to specify how the IP address
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

/// The option to `Server.ftps_required`. It allows the user to specify whether clients are required
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

/// The option to `Server.ftps_client_auth`. Tells if and how mutual TLS (client certificate
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

/// The options for `Server.sitemd5`.
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
