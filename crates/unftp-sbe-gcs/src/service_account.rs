use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::client::HttpConnector;
use hyper::Client;
use hyper_rustls::HttpsConnector;
use libunftp::storage::{Error, ErrorKind};
use yup_oauth2;

#[derive(Clone, Debug)]
pub struct Key(yup_oauth2::ServiceAccountKey);

impl From<yup_oauth2::ServiceAccountKey> for Key {
    fn from(inner: yup_oauth2::ServiceAccountKey) -> Self {
        Key(inner)
    }
}

#[derive(Clone)]
pub struct Authenticator {
    inner_auth: yup_oauth2::authenticator::Authenticator<HttpsConnector<HttpConnector>>,
}

impl Authenticator {
    pub fn new(client: Client<HttpsConnector<HttpConnector>>, key: Key) -> Result<Self, Error> {
        // Spinning up a new runtime is not so nice, but only has to happen once
        let rt = tokio::runtime::Builder::new_current_thread().enable_io().build()?;
        let future_auth = yup_oauth2::ServiceAccountAuthenticator::builder(key.0).hyper_client(client.clone()).build();
        let auth = rt.block_on(future_auth)?;

        Ok(Self { inner_auth: auth })
    }
}

impl std::fmt::Debug for Authenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Authenticator")
    }
}

#[async_trait]
impl TokenProvider for Authenticator {
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
