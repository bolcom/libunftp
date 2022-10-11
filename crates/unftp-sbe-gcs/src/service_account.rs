use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::service::Service;
use hyper::{Client, Uri};
use libunftp::storage::{Error, ErrorKind};
use tokio::io::{AsyncRead, AsyncWrite};
use yup_oauth2;

#[derive(Clone, Debug)]
pub struct Key(yup_oauth2::ServiceAccountKey);

impl From<yup_oauth2::ServiceAccountKey> for Key {
    fn from(inner: yup_oauth2::ServiceAccountKey) -> Self {
        Key(inner)
    }
}

#[derive(Clone)]
pub struct Authenticator<C> {
    inner_auth: yup_oauth2::authenticator::Authenticator<C>,
}

impl<C> Authenticator<C>
where
    C: Clone + Send + Sync + Service<Uri> + 'static,
    C::Response: hyper::client::connect::Connection + AsyncRead + AsyncWrite + Send + Unpin,
    C::Future: Send + Unpin,
    C::Error: Into<Box<dyn std::error::Error + Sync + std::marker::Send + 'static>>,
{
    pub fn new(client: Client<C>, key: Key) -> Result<Self, Error> {
        // Spinning up a new runtime is not so nice, but only has to happen once
        let rt = tokio::runtime::Builder::new_current_thread().enable_io().build()?;
        let future_auth = yup_oauth2::ServiceAccountAuthenticator::builder(key.0).hyper_client(client.clone()).build();
        let auth = rt.block_on(future_auth)?;

        Ok(Self { inner_auth: auth })
    }
}

impl<C> std::fmt::Debug for Authenticator<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Authenticator")
    }
}

#[async_trait]
impl<C> TokenProvider for Authenticator<C>
where
    C: Clone + Send + Sync + Service<Uri> + 'static,
    C::Response: hyper::client::connect::Connection + AsyncRead + AsyncWrite + Send + Unpin,
    C::Future: Send + Unpin,
    C::Error: Into<Box<dyn std::error::Error + Sync + std::marker::Send + 'static>>,
{
    async fn get_token(&self) -> Result<Token, Error> {
        let token = self
            .inner_auth
            .token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .await
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        Ok(Token {
            access_token: token.as_str().to_string(),
            expires_at: token.expiration_time(),
        })
    }
}
