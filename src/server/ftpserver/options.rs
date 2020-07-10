//! Contains code pertaining to the setup options that can be given to the `Server`

use std::ops::Range;
use std::{fmt::Debug, net::Ipv4Addr};

// Once we're sure about the types of these I think its good to expose it to the API user so that
// he/she can see what our server defaults are.
pub(crate) const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
pub(crate) const DEFAULT_IDLE_SESSION_TIMEOUT_SECS: u64 = 600;
pub(crate) const DEFAULT_PASSIVE_HOST: PassiveHost = PassiveHost::FromConnection;
pub(crate) const DEFAULT_PASSIVE_PORTS: Range<u16> = 49152..65535;

/// The option to `Server.passive_host`. It allows the user to specify how the IP address
/// communicated in the _PASV_ response is determined.
#[derive(Debug, PartialEq, Clone)]
pub enum PassiveHost {
    /// Use the IP address of the control connection
    FromConnection,
    /// Advertise this specific IP address
    IP(Ipv4Addr),
    // We also be nice to have:
    // - DNS() to have the server resolve a DNS name on startup and use that IP as the passive IP.
    // - PerUser(Box<dyn (Fn(Box<dyn UserDetail>) -> Ipv4Addr) + Send + Sync>) or something like
    //   that to allow a per user decision

    /// sdsds
    DNS(String)
}

impl Eq for PassiveHost {}

impl From<Ipv4Addr> for PassiveHost {
    fn from(ip: Ipv4Addr) -> Self {
        PassiveHost::IP(ip)
    }
}

impl From<[u8; 4]> for PassiveHost {
    fn from(ip: [u8; 4]) -> Self {
        PassiveHost::IP(ip.into())
    }
}
