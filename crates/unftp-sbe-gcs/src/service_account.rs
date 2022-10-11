use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::client::connect::Connection;
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
    C::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin,
    C::Future: Send + Unpin,
    C::Error: Into<Box<dyn std::error::Error + Sync + std::marker::Send + 'static>>,
{
    pub fn new(client: Client<C>, key: Key) -> Result<Self, Error> {
        let handle = tokio::runtime::Handle::current();

        // Since we're not using yup_oauth2's disk storage, we don't actually do any blocking here.
        let inner_auth = handle.block_on(async move {
            yup_oauth2::ServiceAccountAuthenticator::builder(key.0)
                .hyper_client(client.clone())
                .build()
                .await
                .unwrap()
        });

        Ok(Self { inner_auth })
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
    C::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin,
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

#[cfg(test)]
mod tests {
    use hyper::client::HttpConnector;
    use yup_oauth2::ServiceAccountKey;

    use super::*;

    #[tokio::test]
    async fn get_token() {
        let client = Client::builder().build(HttpConnector::new());
        let key = Key(ServiceAccountKey {
            key_type: None,
            project_id: None,
            private_key_id: None,
            private_key: "".to_string(),
            client_email: "".to_string(),
            client_id: None,
            auth_uri: None,
            token_uri: "".to_string(),
            auth_provider_x509_cert_url: None,
            client_x509_cert_url: None,
        });

        let authenticator = Authenticator::new(client, key).expect("expected authenticator to be instantiated");

        let token = authenticator.get_token().await.unwrap();
        assert_eq!(token.access_token, "".to_string());
        assert_eq!(token.expires_at, None);
    }
}
