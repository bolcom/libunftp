use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::client::connect::Connect;
use hyper::service::Service;
use hyper::{Client, Uri};
use libunftp::storage::{Error, ErrorKind};
use yup_oauth2;

#[derive(Clone, Debug)]
struct Key(yup_oauth2::ServiceAccountKey);

#[derive(Clone, Debug)]
pub struct Authenticator<C> {
    client: Client<C>,
    key: Key,
}

impl<C> Authenticator<C>
where
    C: Sync + Send + Clone + Connect,
{
    pub fn new(client: Client<C>, key: Key) -> Self {
        Self { client: client.clone(), key }
    }
}

#[async_trait]
impl<C> TokenProvider for Authenticator<C>
where
    C: Sync + Send + Clone + Connect,
{
    async fn get_token(&self) -> Result<Token, Error> {
        let inner_auth = yup_oauth2::ServiceAccountAuthenticator::builder(self.key.0)
            .hyper_client(self.client.clone())
            .build()
            .await?;
        let token = inner_auth
            .token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
            .await?;

        Ok(Token {
            access_token: token.access_token,
            expires_at: token.expires_at,
        })
    }
}
