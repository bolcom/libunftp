//! This crate provides a [libunftp](https://crates.io/crates/libunftp) `Authenticator`
//! implementation that authenticates by consuming a JSON REST API.
//!

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::{Method, Request, http::uri::InvalidUri};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex::Regex;
use serde_json::{Value, json};
use unftp_core::auth::{AuthenticationError, Authenticator, Credentials, Principal};

/// A [libunftp](https://crates.io/crates/libunftp) `Authenticator`
/// implementation that authenticates by consuming a JSON REST API.
///
/// [`Authenticator`]: ../spi/trait.Authenticator.html
#[derive(Clone, Debug)]
pub struct RestAuthenticator {
    username_placeholder: String,
    password_placeholder: String,
    source_ip_placeholder: String,

    method: Method,
    url: String,
    body: String,
    selector: String,
    regex: Regex,
}

/// Used to build the [`RestAuthenticator`]
#[derive(Clone, Debug, Default)]
pub struct Builder {
    username_placeholder: String,
    password_placeholder: String,
    source_ip_placeholder: String,

    method: Method,
    url: String,
    body: String,
    selector: String,
    regex: String,
}

impl Builder {
    /// Creates a new `Builder` instance with default settings.
    ///
    /// This method initializes a new builder that you can use to configure and
    /// ultimately construct a [`RestAuthenticator`]. Each setting has a default
    /// value that can be customized through the builder's methods.
    ///
    /// For customization we have several methods:
    /// The placeholder methods (E.g.: `with_username_placeholder`) allow you to
    /// configure placeholders for certain fields.
    /// These placeholders, will be replaced by actual values (FTP username,
    /// password, or the client's source IP) when preparing requests.
    /// You can use these placeholders in the templates supplied `with_url` or
    /// `with_body` .
    ///
    ///
    pub fn new() -> Builder {
        Builder { ..Default::default() }
    }

    /// Sets the placeholder for the FTP username.
    ///
    /// This placeholder will be replaced with the actual FTP username in the fields where it's used.
    /// Refer to the general placeholder concept above for more information.
    ///
    /// # Arguments
    ///
    /// * `s` - A `String` representing the placeholder for the FTP username.
    ///
    /// # Examples
    ///
    /// ```
    /// # use unftp_auth_rest::{Builder, RestAuthenticator};
    /// #
    /// let mut builder = Builder::new()
    ///   .with_username_placeholder("{USER}".to_string())
    ///   .with_body(r#"{"username":"{USER}","password":"{PASS}"}"#.to_string());
    /// ```
    ///
    /// In the example above, `{USER}` within the body template is replaced with the actual FTP username during request
    /// preparation. If the placeholder configuration is not set, any `{USER}` text would stay unreplaced in the request.
    pub fn with_username_placeholder(mut self, s: String) -> Self {
        self.username_placeholder = s;
        self
    }

    /// Sets the placeholder for the FTP password.
    ///
    /// This placeholder will be replaced with the actual FTP password in the fields where it's used.
    /// Refer to the general placeholder concept above for more information.
    ///
    /// # Arguments
    ///
    /// * `s` - A `String` representing the placeholder for the FTP password.
    ///
    /// # Examples
    ///
    /// ```
    /// # use unftp_auth_rest::{Builder, RestAuthenticator};
    /// #
    /// let mut builder = Builder::new()
    ///   .with_password_placeholder("{PASS}".to_string())
    ///   .with_body(r#"{"username":"{USER}","password":"{PASS}"}"#.to_string());
    /// ```
    ///
    /// In the example above, `{PASS}` within the body template is replaced with the actual FTP password during request
    /// preparation. If the placeholder configuration is not set, any `{PASS}` text would stay unreplaced in the request.
    pub fn with_password_placeholder(mut self, s: String) -> Self {
        self.password_placeholder = s;
        self
    }

    /// Sets the placeholder for the source IP of the FTP client.
    ///
    /// This placeholder will be replaced with the actual source IP in the fields where it's used.
    /// Refer to the general placeholder concept above for more information.
    ///
    /// # Arguments
    ///
    /// * `s` - A `String` representing the placeholder for the FTP client's source IP.
    ///
    /// # Examples
    ///
    /// ```
    /// # use unftp_auth_rest::{Builder, RestAuthenticator};
    /// #
    /// let mut builder = Builder::new()
    ///   .with_source_ip_placeholder("{IP}".to_string())
    ///   .with_body(r#"{"username":"{USER}","password":"{PASS}", "source_ip":"{IP}"}"#.to_string());
    /// ```
    ///
    /// In the example above, `{IP}` within the body template is replaced with the actual source IP of the FTP client
    /// during request preparation. If the placeholder configuration is not set, any `{IP}` text would stay unreplaced
    /// in the request.
    pub fn with_source_ip_placeholder(mut self, s: String) -> Self {
        self.source_ip_placeholder = s;
        self
    }

    /// specify HTTP method
    pub fn with_method(mut self, s: Method) -> Self {
        self.method = s;
        self
    }

    /// specify HTTP url
    pub fn with_url(mut self, s: String) -> Self {
        self.url = s;
        self
    }

    /// specify HTTP body (ignored if does not apply for method)
    pub fn with_body(mut self, s: String) -> Self {
        self.body = s;
        self
    }

    /// specify JSON selector to be used to extract the value from the response
    /// format is serde_json's Value.pointer()
    pub fn with_selector(mut self, s: String) -> Self {
        self.selector = s;
        self
    }

    /// specify the value the json selector's result should match to
    pub fn with_regex(mut self, s: String) -> Self {
        self.regex = s;
        self
    }

    /// Creates the authenticator.
    pub fn build(self) -> Result<RestAuthenticator, Box<dyn std::error::Error>> {
        Ok(RestAuthenticator {
            username_placeholder: self.username_placeholder,
            password_placeholder: self.password_placeholder,
            source_ip_placeholder: self.source_ip_placeholder,
            method: self.method,
            url: self.url,
            body: self.body,
            selector: self.selector,
            regex: Regex::new(&self.regex)?,
        })
    }
}

impl RestAuthenticator {
    fn fill_encoded_placeholders(&self, string: &str, username: &str, password: &str, source_ip: &str) -> String {
        let mut result = string.to_owned();

        if !self.username_placeholder.is_empty() {
            result = result.replace(&self.username_placeholder, username);
        }
        if !self.password_placeholder.is_empty() {
            result = result.replace(&self.password_placeholder, password);
        }
        if !self.source_ip_placeholder.is_empty() {
            result = result.replace(&self.source_ip_placeholder, source_ip);
        }

        result
    }
}

trait TrimQuotes {
    fn trim_quotes(&self) -> &str;
}

impl TrimQuotes for String {
    // Used to trim quotes from a json-string formatted string
    fn trim_quotes(&self) -> &str {
        if self.starts_with('"') && self.ends_with('"') && self.len() > 1 {
            &self[1..self.len() - 1]
        } else {
            self
        }
    }
}

#[async_trait]
impl Authenticator for RestAuthenticator {
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, creds: &Credentials) -> Result<Principal, AuthenticationError> {
        let username_url = utf8_percent_encode(username, NON_ALPHANUMERIC).collect::<String>();
        let password = creds.password.as_ref().ok_or(AuthenticationError::BadPassword)?.as_ref();
        let password_url = utf8_percent_encode(password, NON_ALPHANUMERIC).collect::<String>();
        let source_ip = creds.source_ip.to_string();
        let source_ip_url = utf8_percent_encode(&source_ip, NON_ALPHANUMERIC).collect::<String>();

        let url = self.fill_encoded_placeholders(&self.url, &username_url, &password_url, &source_ip_url);

        let username = serde_json::to_string(username)
            .map_err(|e| AuthenticationError::ImplPropagated(e.to_string(), None))?
            .trim_quotes()
            .to_string();
        let password = serde_json::to_string(password)
            .map_err(|e| AuthenticationError::ImplPropagated(e.to_string(), None))?
            .trim_quotes()
            .to_string();
        let source_ip = serde_json::to_string(&source_ip)
            .map_err(|e| AuthenticationError::ImplPropagated(e.to_string(), None))?
            .trim_quotes()
            .to_string();

        let body = self.fill_encoded_placeholders(&self.body, &username, &password, &source_ip);

        let req = Request::builder()
            .method(&self.method)
            .header("Content-type", "application/json")
            .uri(url)
            .body(body)
            .map_err(|e| AuthenticationError::with_source("rest authenticator http client error", e))?;

        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(https);

        let resp = client
            .request(req)
            .await
            .map_err(|e| AuthenticationError::with_source("rest authenticator http client error", e))?;

        let (parts, body) = resp.into_parts();
        let status_context = format!("http status={}", parts.status.as_str());
        let body = BodyExt::collect(body)
            .await
            .map_err(|e| AuthenticationError::with_source(format!("error while receiving http response ({})", status_context), e))?
            .to_bytes();
        let body: Value = serde_json::from_slice(&body)
            .map_err(|e| AuthenticationError::with_source(format!("rest authenticator unmarshalling error ({})", status_context), e))?;

        let parsed = match body.pointer(&self.selector) {
            Some(parsed) => parsed.to_string(),
            None => json!(null).to_string(),
        };

        if self.regex.is_match(&parsed) {
            Ok(Principal {
                username: username.to_string(),
            })
        } else {
            Err(AuthenticationError::BadPassword)
        }
    }
}

/// Possible errors while doing REST lookup
#[allow(missing_docs)] // TODO: fill in these docs
#[derive(Debug)]
pub enum RestError {
    InvalidUri(InvalidUri),
    HttpStatusError(u16),
    HyperError(hyper::Error),
    HttpError(String),
    JsonDeserializationError(serde_json::Error),
    JsonSerializationError(serde_json::Error),
}

impl From<hyper::Error> for RestError {
    fn from(e: hyper::Error) -> Self {
        Self::HttpError(e.to_string())
    }
}

impl From<serde_json::error::Error> for RestError {
    fn from(e: serde_json::error::Error) -> Self {
        Self::JsonDeserializationError(e)
    }
}
