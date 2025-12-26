use super::authenticator::Principal;
use async_trait::async_trait;
use std::{
    fmt::{self, Debug, Display, Formatter},
    path::Path,
};
use thiserror::Error;

/// UserDetail defines the requirements for implementations that hold _Security Subject_
/// information for use by the server.
///
/// Think information like:
///
/// - General information
/// - Account settings
/// - Authorization information
///
/// At this time, this trait doesn't contain much, but it may grow over time to allow for per-user
/// authorization and behaviour.
pub trait UserDetail: Send + Sync + Display + Debug {
    /// Tells if this subject's account is enabled. This default implementation simply returns true.
    fn account_enabled(&self) -> bool {
        true
    }

    /// Returns the user's home directory, if any.  If the user has a home directory, then their
    /// sessions will be restricted to this directory.
    ///
    /// The path should be absolute.
    fn home(&self) -> Option<&Path> {
        None
    }
}

/// Provides a way to convert a [`Principal`] (authenticated identity) into a full [`UserDetail`]
/// implementation with additional user information.
///
/// After authentication returns a [`Principal`], a `UserDetailProvider` can be used to look up
/// additional user details such as home directory, account settings, and authorization information.
/// This separation allows authentication to be decoupled from user detail retrieval.
///
/// # Example
///
/// ```rust
/// use libunftp::auth::{Principal, UserDetail, UserDetailProvider, UserDetailError};
/// use async_trait::async_trait;
///
/// #[derive(Debug)]
/// struct MyUser {
///     username: String,
///     home: Option<std::path::PathBuf>,
/// }
///
/// impl UserDetail for MyUser {
///     fn home(&self) -> Option<&std::path::Path> {
///         self.home.as_deref()
///     }
/// }
///
/// impl std::fmt::Display for MyUser {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.username)
///     }
/// }
///
/// #[derive(Debug)]
/// struct MyUserDetailProvider;
///
/// #[async_trait]
/// impl UserDetailProvider for MyUserDetailProvider {
///     type User = MyUser;
///
///     async fn provide_user_detail(&self, principal: &Principal) -> Result<MyUser, UserDetailError> {
///         // Look up user details from a database or configuration
///         Ok(MyUser {
///             username: principal.username.clone(),
///             home: Some(std::path::PathBuf::from("/home/")),
///         })
///     }
/// }
/// ```
///
/// [`Principal`]: ../struct.Principal.html
/// [`UserDetail`]: trait.UserDetail.html
#[async_trait]
pub trait UserDetailProvider: Send + Sync + Debug {
    /// The `UserDetail` type that this provider returns
    type User: UserDetail;

    /// Converts a [`Principal`] into a full [`UserDetail`] implementation.
    ///
    /// This method should look up additional user information based on the principal's username
    /// and return a complete user detail object.
    ///
    /// # Errors
    ///
    /// Returns [`UserDetailError`] if the user details cannot be retrieved, for example if the
    /// user is not found in the user database.
    ///
    /// [`Principal`]: ../struct.Principal.html
    /// [`UserDetail`]: trait.UserDetail.html
    /// [`UserDetailError`]: enum.UserDetailError.html
    async fn provide_user_detail(&self, principal: &Principal) -> Result<Self::User, UserDetailError>;
}

/// The error type returned by [`UserDetailProvider::provide_user_detail`]
///
/// [`UserDetailProvider`]: trait.UserDetailProvider.html
/// [`UserDetailProvider::provide_user_detail`]: trait.UserDetailProvider.html#tymethod.provide_user_detail
#[derive(Debug, Error)]
pub enum UserDetailError {
    /// A generic error occurred while retrieving user details
    #[error("{0}")]
    Generic(String),
    /// The user was not found in the user database
    #[error("user '{username:?}' not found")]
    UserNotFound {
        /// The username
        username: String,
    },
    /// An implementation-specific error occurred
    #[error("error getting user details: {0}: {1:?}")]
    ImplPropagated(String, #[source] Option<Box<dyn std::error::Error + Send + Sync + 'static>>),
}

impl UserDetailError {
    /// Creates a new domain specific error
    #[allow(dead_code)]
    pub fn new(s: impl Into<String>) -> Self {
        UserDetailError::ImplPropagated(s.into(), None)
    }

    /// Creates a new domain specific error with the given source error.
    pub fn with_source<E>(s: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        UserDetailError::ImplPropagated(s.into(), Some(Box::new(source)))
    }
}

/// DefaultUser is a default implementation of the `UserDetail` trait that doesn't hold any user
/// information. Having a default implementation like this allows for quicker prototyping with
/// libunftp because otherwise the library user would have to implement the `UserDetail` trait first.
#[derive(Debug, PartialEq, Eq)]
pub struct DefaultUser;

impl UserDetail for DefaultUser {}

impl Display for DefaultUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DefaultUser")
    }
}
