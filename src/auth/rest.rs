//! [`Authenticator`] implementation that authenticates against a JSON REST API.
//!
//! [`Authenticator`]: trait.Authenticator.html

use crate::auth::*;
use async_trait::async_trait;
use hyper::{http::uri::InvalidUri, Body, Client, Method, Request};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use serde_json::{json, Value};
use std::string::String;

/// [`Authenticator`] implementation that authenticates against a JSON REST API.
///
/// [`Authenticator`]: ../spi/trait.Authenticator.html
#[derive(Clone, Debug)]
pub struct RestAuthenticator {
    username_placeholder: String,
    password_placeholder: String,

    method: Method,
    url: String,
    body: String,
    selector: String,
    regex: Regex,
}

///
#[derive(Clone, Debug, Default)]
pub struct Builder {
    username_placeholder: String,
    password_placeholder: String,

    method: Method,
    url: String,
    body: String,
    selector: String,
    regex: String,
}

impl Builder {
    ///
    pub fn new() -> Builder {
        Builder { ..Default::default() }
    }

    /// specify the placeholder string in the rest of the fields that would be replaced by the username
    pub fn with_username_placeholder(mut self, s: String) -> Self {
        self.username_placeholder = s;
        self
    }

    /// specify the placeholder string in the rest of the fields that would be replaced by the password
    pub fn with_password_placeholder(mut self, s: String) -> Self {
        self.password_placeholder = s;
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

    ///
    pub fn build(self) -> Result<RestAuthenticator, Box<dyn std::error::Error>> {
        Ok(RestAuthenticator {
            username_placeholder: self.username_placeholder,
            password_placeholder: self.password_placeholder,
            method: self.method,
            url: self.url,
            body: self.body,
            selector: self.selector,
            regex: Regex::new(&self.regex)?,
        })
    }
}

impl RestAuthenticator {
    fn fill_encoded_placeholders(&self, string: &str, username: &str, password: &str) -> String {
        string
            .replace(&self.username_placeholder, username)
            .replace(&self.password_placeholder, password)
    }
}

// FIXME: add support for authenticated user
#[async_trait]
impl Authenticator<DefaultUser> for RestAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, password: &str) -> Result<DefaultUser, Box<dyn std::error::Error + Send + Sync>> {
        let username_url = utf8_percent_encode(username, NON_ALPHANUMERIC).collect::<String>();
        let password_url = utf8_percent_encode(password, NON_ALPHANUMERIC).collect::<String>();
        let url = self.fill_encoded_placeholders(&self.url, &username_url, &password_url);

        let username_json = encode_string_json(username);
        let password_json = encode_string_json(password);
        let body = self.fill_encoded_placeholders(&self.body, &username_json, &password_json);

        // FIXME: need to clone too much, just to keep tokio::spawn() happy, with its 'static requirement. is there a way maybe to work this around with proper lifetime specifiers? Or is it better to just clone the whole object?
        let method = self.method.clone();
        let selector = self.selector.clone();
        let regex = self.regex.clone();

        debug!("{} {}", url, body);

        let req = Request::builder()
            .method(method)
            .header("Content-type", "application/json")
            .uri(url)
            .body(Body::from(body))?;

        let client = Client::new();

        let resp = client.request(req).await?;
        let body_bytes = hyper::body::to_bytes(resp.into_body()).await?;

        let body: Value = serde_json::from_slice(&body_bytes)?;
        let parsed = match body.pointer(&selector) {
            Some(parsed) => parsed.to_string(),
            None => json!(null).to_string(),
        };

        if regex.is_match(&parsed) {
            Ok(DefaultUser {})
        } else {
            Err(Box::new(BadPasswordError))
        }
    }
}

/// limited capabilities, meant for us-ascii username and password only, really
fn encode_string_json(string: &str) -> String {
    let mut res = String::with_capacity(string.len() * 2);

    for i in string.chars() {
        match i {
            '\\' => res.push_str("\\\\"),
            '"' => res.push_str("\\\""),
            ' '..='~' => res.push(i),
            _ => {
                error!("special character {} is not supported", i);
            }
        }
    }

    res
}

/// Possible errors while doing REST lookup
#[derive(Debug)]
pub enum RestError {
    ///
    InvalidUri(InvalidUri),
    ///
    HTTPStatusError(u16),
    ///
    HyperError(hyper::error::Error),
    ///
    HttpError(String),
    ///
    JSONDeserializationError(serde_json::Error),
    ///
    JSONSerializationError(serde_json::Error),
}

impl From<hyper::error::Error> for RestError {
    fn from(e: hyper::error::Error) -> Self {
        Self::HttpError(e.to_string())
    }
}

impl From<serde_json::error::Error> for RestError {
    fn from(e: serde_json::error::Error) -> Self {
        Self::JSONDeserializationError(e)
    }
}
