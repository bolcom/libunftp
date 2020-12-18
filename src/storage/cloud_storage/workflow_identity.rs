// TODO: Test whether on GCE
// See https://github.com/mechiru/gcemeta/blob/master/src/metadata.rs
// See https://github.com/mechiru/gouth/blob/master/gouth/src/source/metadata.rs

use crate::storage::{Error, ErrorKind};
use hyper::client::connect::dns::GaiResolver;
use hyper::client::HttpConnector;
use hyper::http::header;
use hyper::{Body, Client, Method, Request, Response};
use hyper_rustls::HttpsConnector;

// Environment variable specifying the GCE metadata hostname.
// If empty, the default value of `METADATA_IP` is used instead.
// const METADATA_HOST_VAR: &str = "GCE_METADATA_HOST";

// Documented metadata server IP address.
// const METADATA_IP: &str = "169.254.169.254";

// When is using the IP better?
const METADATA_HOST: &str = "metadata.google.internal";

// `github.com/bolcom/libunftp v{package_version}`
const USER_AGENT: &str = concat!("github.com/bolcom/libunftp v", env!("CARGO_PKG_VERSION"));

// TODO: MAP to useful error type
// TODO: Cache the token.
pub(super) async fn request_token(service: Option<String>, client: Client<HttpsConnector<HttpConnector<GaiResolver>>>) -> Result<TokenResponse, Error> {
    // Does same as curl -s -HMetadata-Flavor:Google http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
    let suffix = format!("instance/service-accounts/{}/token", service.unwrap_or_else(|| "default".to_string()));
    //let host = env::var(METADATA_HOST_VAR).unwrap_or_else(|_| METADATA_IP.into());
    let host = METADATA_HOST;
    let uri = format!("http://{}/computeMetadata/v1/{}", host, suffix);

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

    let unmarshall_result: serde_json::Result<TokenResponse> = serde_json::from_slice(body_bytes.to_vec().as_slice());
    unmarshall_result.map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
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
    pub(super) expires_in: u64,
}
