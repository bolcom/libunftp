use crate::auth::*;

use regex::Regex;
use std::string::String;

use async_trait::async_trait;

use futures_util::future::FutureExt;
use futures_util::future::TryFutureExt;

use http::uri::InvalidUri;

use bytes::Bytes;

use hyper::body::HttpBody;
use hyper::{Body, Client, Request};

use serde_json::Value;
use url::percent_encoding::{utf8_percent_encode, PATH_SEGMENT_ENCODE_SET};

/// [`Authenticator`] implementation that authenticates against a JSON REST API.
///
/// [`Authenticator`]: ../trait.Authenticator.html
#[derive(Clone, Debug)]
pub struct RestAuthenticator {
    username_placeholder: String,
    password_placeholder: String,

    method: http::Method,
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

    method: http::Method,
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
    pub fn with_method(mut self, s: http::Method) -> Self {
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
    pub fn build(self) -> RestAuthenticator {
        RestAuthenticator {
            username_placeholder: self.username_placeholder,
            password_placeholder: self.password_placeholder,
            method: self.method,
            url: self.url,
            body: self.body,
            selector: self.selector,
            regex: Regex::new(&self.regex).unwrap(),
        }
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
impl Authenticator<AnonymousUser> for RestAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<AnonymousUser, ()> {
        self.authenticate_rest(username, password)
            .map_err(|err| {
                info!("RestError: {:?}", err);
            })
            .await
    }
}

impl RestAuthenticator {
    async fn authenticate_rest(&self, username: &str, password: &str) -> Result<AnonymousUser, RestError> {
        let username_url = utf8_percent_encode(username, PATH_SEGMENT_ENCODE_SET).collect::<String>();
        let password_url = utf8_percent_encode(password, PATH_SEGMENT_ENCODE_SET).collect::<String>();
        let url = self.fill_encoded_placeholders(&self.url, &username_url, &password_url);

        let username_json = encode_string_json(username);
        let password_json = encode_string_json(password);
        let body = self.fill_encoded_placeholders(&self.body, &username_json, &password_json);

        // FIXME: need to clone too much, just to keep tokio::spawn() happy,
        // with its 'static requirement. is there a way maybe to work this
        // around with proper lifetime specifiers? Or is it better to just clone
        // the whole object?
        let selector = self.selector.clone();
        let regex = self.regex.clone();

        debug!("{} {}", url, body);

        let request = Request::builder()
            .method(self.method)
            .header("Content-type", "application/json")
            .uri(url)
            .body(Body::from(body))
            .map_err(|e| RestError::HttpError(e.to_string()))?;

        let res = Client::new().request(request).await?;
        let bs = hyper::body::to_bytes(res.body_mut()).await?;
        let response: Value = serde_json::from_slice(&bs)?;

        let parsed = response
            .pointer(&selector)
            .map(|x| {
                debug!("pointer: {:?}", x);
                format!("{:?}", x)
            })
            .unwrap_or_else(|| "null".to_string());

        if regex.is_match(&parsed) {
            Ok(AnonymousUser {})
        } else {
            Err(RestError::HttpError("unauthorized".into()))
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
