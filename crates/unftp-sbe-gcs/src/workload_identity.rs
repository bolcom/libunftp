// TODO: Test whether on GCE
// See https://github.com/mechiru/gcemeta/blob/master/src/metadata.rs
// See https://github.com/mechiru/gouth/blob/master/gouth/src/source/metadata.rs

use crate::auth::{Token, TokenProvider};
use async_trait::async_trait;
use hyper::client::connect::Connect;
use hyper::http::header;
use hyper::{Body, Client, Method, Request, Response};
use libunftp::storage::{Error, ErrorKind};

// Environment variable specifying the GCE metadata hostname.
// If empty, the default value of `METADATA_IP` is used instead.
// const METADATA_HOST_VAR: &str = "GCE_METADATA_HOST";

// Documented metadata server IP address.
// const METADATA_IP: &str = "169.254.169.254";

// When is using the IP better?
const METADATA_HOST: &str = "metadata.google.internal";

// `github.com/bolcom/libunftp v{package_version}`
const USER_AGENT: &str = concat!("github.com/bolcom/libunftp v", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub struct WorkloadIdentity {
    service: String,
}

impl WorkloadIdentity {
    fn new(service: String) -> Self {
        Self { service }
    }
}

impl Default for WorkloadIdentity {
    fn default() -> Self {
        Self {
            service: "default".to_string(),
        }
    }
}

#[async_trait]
impl TokenProvider for WorkloadIdentity {
    // TODO: MAP to useful error type
    async fn get_token<C>(&self, client: Client<C>) -> Result<Token, Error>
    where
        C: Sync + Send + Clone + Connect,
    {
        // Does same as curl -s -HMetadata-Flavor:Google http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
        let host = METADATA_HOST;
        let uri = format!("http://{}/computeMetadata/v1/instance/service-accounts/{}/token", host, self.service);

        let now = time::OffsetDateTime::now_utc();
        let request = Request::builder()
            .uri(uri)
            .header("Metadata-Flavor", "Google")
            .header(header::USER_AGENT, USER_AGENT)
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).await.map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let body_bytes = hyper::body::to_bytes(response.into_body())
            .await
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let token: serde_json::Result<TokenResponse> = serde_json::from_slice(body_bytes.to_vec().as_slice());
        let token = token.map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let expiry_deadline = now.saturating_add(time::Duration::seconds(token.expires_in));

        Ok(Token {
            access_token: token.access_token,
            expires_at: Some(expiry_deadline),
        })
    }
}

// Example:
// ```
// {
//   "access_token": "ya29.c.Ks0Cywchw6EJei_7ifQZKV....oRZy70M2ahRMfHY1qzUxGfxQcQ1cQ",
//   "expires_in": 3166,
//   "token_type": "Bearer"
// }
// ```
//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct TokenResponse {
    pub(super) token_type: String,
    pub(super) access_token: String,
    pub(super) expires_in: i64,
}
