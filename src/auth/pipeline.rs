//! Authentication pipeline that combines authentication and user detail retrieval.

use super::{
    authenticator::{AuthenticationError, Authenticator, Credentials},
    user::{UserDetail, UserDetailProvider},
};
use std::fmt::Debug;
use std::sync::Arc;

/// Combines an [`Authenticator`] and a [`UserDetailProvider`] to provide a complete
/// authentication flow that returns a full [`UserDetail`] implementation.
///
/// This pipeline encapsulates the two-step process:
/// 1. Authenticate the user (returns `Principal`)
/// 2. Retrieve user details (converts `Principal` to `User: UserDetail`)
///
/// # Example
///
/// ```rust
/// use libunftp::auth::{AuthenticationPipeline, Authenticator, UserDetailProvider, Principal, DefaultUser};
/// use async_trait::async_trait;
/// use std::sync::Arc;
///
/// // Assuming you have a non-generic Authenticator and a UserDetailProvider
/// // let authenticator = Arc::new(MyAuthenticator);
/// // let user_provider = Arc::new(MyUserDetailProvider);
/// // let pipeline = AuthenticationPipeline::new(authenticator, user_provider);
/// ```
///
/// [`Authenticator`]: trait.Authenticator.html
/// [`UserDetailProvider`]: trait.UserDetailProvider.html
/// [`UserDetail`]: trait.UserDetail.html
#[derive(Debug)]
pub struct AuthenticationPipeline<User>
where
    User: UserDetail,
{
    authenticator: Arc<dyn Authenticator + Send + Sync>,
    user_provider: Arc<dyn UserDetailProvider<User = User> + Send + Sync>,
}

impl<User> AuthenticationPipeline<User>
where
    User: UserDetail,
{
    /// Creates a new `AuthenticationPipeline` combining the given authenticator and user provider.
    ///
    /// The authenticator returns a `Principal` and the provider converts it to a full `UserDetail`.
    pub fn new(authenticator: Arc<dyn Authenticator + Send + Sync>, user_provider: Arc<dyn UserDetailProvider<User = User> + Send + Sync>) -> Self {
        Self { authenticator, user_provider }
    }

    /// Returns a reference to the underlying authenticator.
    pub fn authenticator(&self) -> &Arc<dyn Authenticator + Send + Sync> {
        &self.authenticator
    }

    /// Returns a reference to the underlying user detail provider.
    pub fn user_provider(&self) -> &Arc<dyn UserDetailProvider<User = User> + Send + Sync> {
        &self.user_provider
    }

    /// Authenticates the user and returns the full user detail.
    ///
    /// This method will:
    /// 1. Authenticate the user (to verify credentials and get a `Principal`)
    /// 2. Use the provider to convert the authenticated `Principal` to a full `UserDetail`
    ///
    /// # Errors
    ///
    /// Returns `AuthenticationError` if authentication fails or if user detail retrieval fails.
    pub async fn authenticate_and_get_user(&self, username: &str, creds: &Credentials) -> Result<User, AuthenticationError> {
        // Authenticate to get Principal
        let principal = self.authenticator.authenticate(username, creds).await?;

        // Use the provider to convert Principal to User
        self.user_provider.provide_user_detail(&principal).await.map_err(|e| match e {
            super::user::UserDetailError::UserNotFound { .. } => AuthenticationError::BadUser,
            super::user::UserDetailError::Generic(msg) => AuthenticationError::new(msg),
            super::user::UserDetailError::ImplPropagated(msg, source) => AuthenticationError::ImplPropagated(msg, source),
        })
    }

    /// Tells whether its OK to not ask for a password when a valid client cert
    /// was presented.
    ///
    /// This delegates to the underlying authenticator's `cert_auth_sufficient` method.
    pub async fn cert_auth_sufficient(&self, username: &str) -> bool {
        self.authenticator.cert_auth_sufficient(username).await
    }

    /// Returns the name of the authenticator.
    ///
    /// This delegates to the underlying authenticator's `name` method.
    pub fn name(&self) -> &str {
        self.authenticator.name()
    }
}
