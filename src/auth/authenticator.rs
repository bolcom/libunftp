//! The service provider interface (SPI) for auth

use super::UserDetail;

use async_trait::async_trait;
use std::error::Error;
use std::fmt;

/// Defines the requirements for Authentication implementations
#[async_trait]
pub trait Authenticator<U>: Sync + Send
where
    U: UserDetail,
{
    /// Authenticate the given user with the given password.
    async fn authenticate(&self, username: &str, password: &str) -> Result<U, Box<dyn std::error::Error + Send + Sync>>;
}

#[derive(Debug)]
pub(crate) struct BadPasswordError;

impl fmt::Display for BadPasswordError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "bad password")
    }
}

impl Error for BadPasswordError {}

#[derive(Debug)]
pub(crate) struct UnknownUsernameError;

impl fmt::Display for UnknownUsernameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "unknown user")
    }
}

impl Error for UnknownUsernameError {}
