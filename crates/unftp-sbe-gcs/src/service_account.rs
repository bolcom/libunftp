use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::client::connect::Connect;
use hyper::service::Service;
use hyper::{Client, Uri};
use libunftp::storage::{Error, ErrorKind};
use yup_oauth2;

struct Key(yup_oauth2::ServiceAccountKey);

#[derive(Clone)]
pub struct Authenticator {
    inner_auth: yup_oauth2::ServiceAccountAuthenticator,
}

impl Authenticator {
    pub async fn new<C>(client: Client<C>, key: Key) -> Self
    where
        C: Sync + Send + Clone + Service<Uri> + Connect,
    {
        let inner_auth = yup_oauth2::ServiceAccountAuthenticator::builder(key.0)
            .hyper_client(client.clone())
            .build()
            .await;
        Self { inner_auth }
    }
}

#[async_trait]
impl TokenProvider for Authenticator {
    async fn get_token(&self) -> Result<Token, Error> {
        let token = self
            .inner_auth
            .token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
            .await?;

        Ok(Token {
            access_token: token.access_token,
            expires_at: token.expires_at,
        })
    }
}
