//! Contains the error type used by `Server`

use crate::BoxError;

use std::net::AddrParseError;
use thiserror::Error;

/// Error returned by the [`Server.listen`](crate::Server::listen()) method
#[derive(Error, Debug)]
#[error("server error: {msg}")]
pub struct ServerError {
    msg: String,
    #[source]
    source: BoxError,
}

impl ServerError {
    fn new<E: std::error::Error + Send + Sync + 'static>(msg: impl Into<String>, source: E) -> ServerError {
        ServerError {
            msg: msg.into(),
            source: Box::new(source),
        }
    }
}

impl From<AddrParseError> for ServerError {
    fn from(e: AddrParseError) -> Self {
        ServerError::new("could not parse address", e)
    }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        ServerError::new("io error", e)
    }
}

impl From<super::tls::ConfigError> for ServerError {
    fn from(e: super::tls::ConfigError) -> Self {
        ServerError::new(format!("error with TLS configuration: {}", e), e)
    }
}

#[derive(Error, Debug)]
#[error("shutdown error: {msg}")]
pub struct ShutdownError {
    pub msg: String,
}

impl From<ShutdownError> for ServerError {
    fn from(e: ShutdownError) -> Self {
        ServerError::new("shutdown error", e)
    }
}
