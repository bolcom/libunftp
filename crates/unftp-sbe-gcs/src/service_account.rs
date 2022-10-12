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
    client: Client<C>,
    key: Key,
}

impl<C> Authenticator<C> {
    pub fn new(client: Client<C>, key: Key) -> Self {
        Self { client, key }
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
        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(self.key.0.clone())
            .hyper_client(self.client.clone())
            .build()
            .await?;

        let token = auth
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

    // TODO: Uncomment after figuring out a way to test this
    // #[tokio::test]
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

        let authenticator = Authenticator::new(client, key);

        let token = authenticator.get_token().await.unwrap();

        dbg!(&token);
        assert_eq!(token.access_token, "".to_string());
        assert_eq!(token.expires_at, None);
    }
}
