//! The service provider interface (SPI) for auth

use super::UserDetail;
use crate::BoxError;

use async_trait::async_trait;
use std::fmt::{Debug, Formatter};
use thiserror::Error;

/// Defines the requirements for Authentication implementations
#[async_trait]
pub trait Authenticator<User>: Sync + Send + Debug
where
    User: UserDetail,
{
    /// Authenticate the given user with the given credentials.
    async fn authenticate(&self, username: &str, creds: &Credentials) -> Result<User, AuthenticationError>;

    /// Tells whether its OK to not ask for a password when a valid client cert
    /// was presented.
    async fn cert_auth_sufficient(&self, _username: &str) -> bool {
        false
    }

    /// Implement to set the name of the authenticator. By default it returns the type signature.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Represents an authenticated principal (user identity) returned by an [`Authenticator`].
///
/// A `Principal` contains the authenticated username and is the result of successful authentication.
/// It represents the minimal identity information needed after authentication. To obtain additional
/// user information such as home directory and account settings, use a [`UserDetailProvider`] to
/// convert the `Principal` into a full [`UserDetail`] implementation.
///
/// # Example
///
/// ```rust
/// use libunftp::auth::Principal;
///
/// let principal = Principal {
///     username: "alice".to_string(),
/// };
///
/// assert_eq!(principal.username, "alice");
/// ```
///
/// [`Authenticator`]: trait.Authenticator.html
/// [`UserDetail`]: ../trait.UserDetail.html
/// [`UserDetailProvider`]: ../trait.UserDetailProvider.html
#[derive(Debug, Clone)]
pub struct Principal {
    /// The authenticated username
    pub username: String,
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

    /// A bad client certificate was presented.
    #[error("bad client certificate")]
    BadCert,

    /// The source IP address was not allowed
    #[error("client IP address not allowed")]
    IpDisallowed,

    /// The certificate CN is not allowed for this user
    #[error("certificate does not match allowed CN for this user")]
    CnDisallowed,

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

/// Credentials passed to an [Authenticator](crate::auth::Authenticator)
///
/// [Authenticator](crate::auth::Authenticator) implementations can assume that either `certificate_chain` or `password`
/// will not be `None`.
#[derive(Clone, Debug)]
pub struct Credentials {
    /// The password that the client sent.
    pub password: Option<String>,
    /// DER encoded x509 certificate chain coming from the client.
    pub certificate_chain: Option<Vec<ClientCert>>,
    /// The IP address of the user's connection
    pub source_ip: std::net::IpAddr,
}

impl From<&str> for Credentials {
    fn from(s: &str) -> Self {
        Credentials {
            password: Some(String::from(s)),
            certificate_chain: None,
            source_ip: [127, 0, 0, 1].into(),
        }
    }
}

/// Contains a single DER-encoded X.509 client certificate.
#[derive(Clone, Eq, PartialEq)]
pub struct ClientCert(pub Vec<u8>);

use x509_parser::prelude::parse_x509_certificate;

impl ClientCert {
    /// Returns true if the Common Name from the client certificate matches the allowed_cn
    pub fn verify_cn(&self, allowed_cn: &str) -> Result<bool, std::io::Error> {
        let client_cert = parse_x509_certificate(&self.0);
        let subject = match client_cert {
            Ok(c) => c.1.subject().to_string(),
            Err(e) => return Err(std::io::Error::other(e.to_string())),
        };

        Ok(subject.contains(allowed_cn))
    }
}

impl std::fmt::Debug for ClientCert {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientCert(***)")
    }
}

impl AsRef<[u8]> for ClientCert {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
