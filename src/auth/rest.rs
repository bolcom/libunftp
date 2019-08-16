use crate::auth::Authenticator;

use regex::Regex;
use std::result::Result;
use std::string::String;

use futures::stream::Stream;
use futures::Future;
use tokio::runtime::current_thread::Runtime;

use http::uri::InvalidUri;
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
        Builder {
            ..Default::default()
        }
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

impl Authenticator for RestAuthenticator {
    fn authenticate(
        &self,
        _username: &str,
        _password: &str,
    ) -> Box<Future<Item = bool, Error = ()> + Send> {
        let username_url =
            utf8_percent_encode(_username, PATH_SEGMENT_ENCODE_SET).collect::<String>();
        let password_url =
            utf8_percent_encode(_password, PATH_SEGMENT_ENCODE_SET).collect::<String>();
        let url = self.fill_encoded_placeholders(&self.url, &username_url, &password_url);

        let username_json = encode_string_json(_username);
        let password_json = encode_string_json(_password);
        let body = self.fill_encoded_placeholders(&self.body, &username_json, &password_json);

        // FIXME: need to clone too much, just to keep tokio::spawn() happy, with its 'static requirement. is there a way maybe to work this around with proper lifetime specifiers? Or is it better to just clone the whole object?
        let method = self.method.clone();
        let selector = self.selector.clone();
        let regex = self.regex.clone();

        debug!("{} {}", url, body);

        Box::new(
            futures::future::ok(())
                .and_then(|_| {
                    Request::builder()
                        .method(method)
                        .header("Content-type", "application/json")
                        .uri(url)
                        .body(Body::from(body))
                        .map_err(|e| RestError::HttpError(e.to_string()))
                })
                .and_then(|req| Client::new().request(req).map_err(RestError::HyperError))
                .and_then(|res| res.into_body().map_err(RestError::HyperError).concat2())
                .and_then(|body| {
                    //                println!("resp: {:?}", body);
                    serde_json::from_slice(&body).map_err(RestError::JSONDeserializationError)
                })
                .and_then(move |response: Value| {
                    let parsed = response
                        .pointer(&selector)
                        .map(|x| {
                            //                        println!("pointer: {:?}", x);
                            format!("{:?}", x)
                        })
                        .unwrap_or("null".to_string());
                    Result::Ok(regex.is_match(&parsed))
                })
                .map_err(|err| {
                    // FIXME: log error
                    //                println!("RestError: {:?}", err);
                    ()
                }),
        )
    }
}

/// limited capabilities, meant for us-ascii username and password only, really
fn encode_string_json(string: &str) -> String {
    let mut res = String::with_capacity(string.len() * 2);

    for i in string.chars() {
        match i {
            '\\' => res.push_str("\\\\"),
            '"' => res.push_str("\\\""),
            ' '...'~' => res.push(i),
            _ => {
                // FIXME: no support for non-ASCII right now
            }
        }
    }

    return res;
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
