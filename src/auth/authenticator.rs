//! The service provider interface (SPI) for auth

use super::UserDetail;
use async_trait::async_trait;
use std::{
    error::Error,
    fmt::{self, Debug},
};

/// Defines the requirements for Authentication implementations
#[async_trait]
pub trait Authenticator<U>: Sync + Send + Debug
where
    U: UserDetail,
{
    /// Authenticate the given user with the given password.
    async fn authenticate(&self, username: &str, password: &str) -> Result<U, AuthenticationError>;
}

/// The error type for authentication errors
#[derive(Debug)]
pub struct AuthenticationError;

impl fmt::Display for AuthenticationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Authentication error")
    }
}

impl Error for AuthenticationError {}

impl std::convert::From<std::io::Error> for AuthenticationError {
    fn from(_: std::io::Error) -> Self {
        AuthenticationError
    }
}
