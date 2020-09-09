//! The service provider interface (SPI) for auth

use super::UserDetail;
use crate::BoxError;

use async_trait::async_trait;
use std::fmt::Debug;
use thiserror::Error;

/// Defines the requirements for Authentication implementations
#[async_trait]
pub trait Authenticator<U>: Sync + Send + Debug
where
    U: UserDetail,
{
    /// Authenticate the given user with the given password.
    async fn authenticate(&self, username: &str, password: &str) -> Result<U, AuthenticationError>;
}

/// The error type returned by `Authenticator.authenticate`
#[derive(Error, Debug)]
pub enum AuthenticationError {
    /// A bad password was provided
    #[error("bad password")]
    BadPassword,

    /// A bad username was provided
    #[error("bad username")]
    BadUser,

    /// Another issue occurred during the authentication process.
    #[error("authentication error: {0}: {1:?}")]
    ImplPropagated(String, #[source] Option<BoxError>),
}

impl AuthenticationError {
    /// Creates a new domain specific error
    pub fn new(s: impl Into<String>) -> AuthenticationError {
        AuthenticationError::ImplPropagated(s.into(), None)
    }

    /// Creates a new domain specific error with the given source error.
    pub fn with_source<E>(s: impl Into<String>, source: E) -> AuthenticationError
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        AuthenticationError::ImplPropagated(s.into(), Some(Box::new(source)))
    }
}
