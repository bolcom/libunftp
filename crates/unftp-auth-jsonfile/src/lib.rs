//! This crate implements a libunftp `Authenticator` that authenticates against credentials in a JSON formatted file.
//!
//! It supports both plaintext as well as [PBKDF2](https://tools.ietf.org/html/rfc2898#section-5.2) encoded passwords.
//!
//! # Plaintext example
//!
//! ```json
//! [
//!   {
//!     "username": "alice",
//!     "password": "I am in Wonderland!"
//!   }
//! ]
//! ```
//!
//! # PBKDF2 encoded Example
//!
//! Both the salt and key need to be base64 encoded.
//! Currently only HMAC_SHA256 is supported by libunftp (more will be supported later).
//!
//! There are various tools that can be used to generate the key.
//! In this example, we use [nettle-pbkdf2](http://www.lysator.liu.se/~nisse/nettle/) which can generate the HMAC_SHA256.
//!
//! Generate a secure salt:
//! ```sh
//! salt=$(dd if=/dev/random bs=1 count=8)
//! ```
//!
//! Generate the base64 encoded PBKDF2 key, to be copied into the `pbkdf2_key` field of the JSON structure.
//! Make sure however to not exceed the output length of the digest algorithm (256 bit, 32 bytes in our case):
//! ```sh
//! echo -n "mypassword" | nettle-pbkdf2 -i 500000 -l 32 --hex-salt $(echo -n $salt | xxd -p -c 80) --raw |openssl base64 -A
//! ```
//!
//! Convert the salt into base64 to be copied into the `pbkdf2_salt` field of the JSON structure:
//! ```sh
//! echo -n $salt | openssl base64 -A
//! ```
//!
//! Now write these to the JSON file, as seen below. Make sure that `pbkdf2_iter` matches the iterations (`-i`) used with `nettle-pbkdf2`.
//!
//! ```json
//! [
//!   {
//!     "username": "bob",
//!     "pbkdf2_salt": "<<BASE_64_RANDOM_SALT>>",
//!     "pbkdf2_key": "<<BASE_64_KEY>>",
//!     "pbkdf2_iter": 500000
//!   },
//! ]
//! ```
//!
//! # Mixed example
//!
//! It is possible to mix plaintext and pbkdf2 encoded type passwords.
//!
//! ```json
//! [
//!   {
//!     "username": "alice",
//!     "pbkdf2_salt": "<<BASE_64_RANDOM_SALT>>",
//!     "pbkdf2_key": "<<BASE_64_KEY>>",
//!     "pbkdf2_iter": 500000
//!   },
//!   {
//!     "username": "bob",
//!     "password": "This password is a joke"
//!   }
//! ]
//! ```
//!
//! # Preventing unauthorized access with allow lists
//!
//! ```json
//! [
//!   {
//!     "username": "bob",
//!     "password": "it is me",
//!     "allowed_ip_ranges": ["192.168.178.0/24", "127.0.0.0/8"]
//!   },
//! ]
//! ```
//!
//! # Using it with libunftp
//!
//! Use [JsonFileAuthenticator::from_file](crate::JsonFileAuthenticator::from_file) to load the JSON structure directly from a file.
//! See the example `examples/jsonfile_auth.rs`.
//!
//! Alternatively use another source for your JSON credentials, and use [JsonFileAuthenticator::from_json](crate::JsonFileAuthenticator::from_json) instead.

use async_trait::async_trait;
use bytes::Bytes;
use ipnet::Ipv4Net;
use iprange::IpRange;
use libunftp::auth::{AuthenticationError, Authenticator, DefaultUser};
use ring::{
    digest::SHA256_OUTPUT_LEN,
    pbkdf2::{verify, PBKDF2_HMAC_SHA256},
};
use serde::Deserialize;
use std::{collections::HashMap, convert::TryInto, fs, num::NonZeroU32, path::Path, time::Duration};
use tokio::time::sleep;
use valid::{constraint::Length, Validate};

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum Credentials {
    Plaintext {
        username: String,
        password: String,
        allowed_ip_ranges: Option<Vec<String>>,
    },
    Pbkdf2 {
        username: String,
        pbkdf2_salt: String,
        pbkdf2_key: String,
        pbkdf2_iter: NonZeroU32,
        allowed_ip_ranges: Option<Vec<String>>,
    },
}

/// This structure implements the libunftp `Authenticator` trait
#[derive(Clone, Debug)]
pub struct JsonFileAuthenticator {
    credentials_map: HashMap<String, UserCreds>,
}

#[derive(Clone, Debug)]
enum Password {
    PlainPassword {
        password: String,
    },
    Pbkdf2Password {
        pbkdf2_salt: Bytes,
        pbkdf2_key: Bytes,
        pbkdf2_iter: NonZeroU32,
    },
}

#[derive(Clone, Debug)]
struct UserCreds {
    pub password: Password,
    pub allowed_ip_ranges: Option<IpRange<Ipv4Net>>,
}

impl JsonFileAuthenticator {
    /// Initialize a new [`JsonFileAuthenticator`] from file.
    pub fn from_file<P: AsRef<Path>>(filename: P) -> Result<Self, Box<dyn std::error::Error>> {
        let json: String = fs::read_to_string(filename)?;
        Self::from_json(json)
    }

    /// Initialize a new [`JsonFileAuthenticator`] from json string.
    pub fn from_json<T: Into<String>>(json: T) -> Result<Self, Box<dyn std::error::Error>> {
        let credentials_list: Vec<Credentials> = serde_json::from_str::<Vec<Credentials>>(&json.into())?;
        let map: Result<HashMap<String, UserCreds>, _> = credentials_list.into_iter().map(Self::list_entry_to_map_entry).collect();
        Ok(JsonFileAuthenticator { credentials_map: map? })
    }

    fn list_entry_to_map_entry(user_info: Credentials) -> Result<(String, UserCreds), Box<dyn std::error::Error>> {
        let map_entry = match user_info {
            Credentials::Plaintext {
                username,
                password,
                allowed_ip_ranges: ip_ranges,
            } => (
                username.clone(),
                UserCreds {
                    password: Password::PlainPassword { password },
                    allowed_ip_ranges: Self::parse_ip_range(username, ip_ranges)?,
                },
            ),
            Credentials::Pbkdf2 {
                username,
                pbkdf2_salt,
                pbkdf2_key,
                pbkdf2_iter,
                allowed_ip_ranges: ip_ranges,
            } => (
                username.clone(),
                UserCreds {
                    password: Password::Pbkdf2Password {
                        pbkdf2_salt: base64::decode(pbkdf2_salt)
                            .map_err(|_| "Could not base64 decode the salt")?
                            .try_into()
                            .map_err(|_| "Could not convert String to Bytes")?,
                        pbkdf2_key: base64::decode(pbkdf2_key)
                            .map_err(|_| "Could not decode base64")?
                            .validate("pbkdf2_key", &Length::Max(SHA256_OUTPUT_LEN))
                            .result()
                            .map_err(|_| {
                                format!("Key of user \"{}\" is too long", username)
                            })?
                            .unwrap() // this unwrap is just giving the value within
                            .try_into()
                            .map_err(|_| "Could not convert to Bytes")?,
                        pbkdf2_iter,
                    },
                    allowed_ip_ranges: Self::parse_ip_range(username, ip_ranges)?,
                },
            ),
        };
        Ok(map_entry)
    }

    fn parse_ip_range(username: String, ip_ranges: Option<Vec<String>>) -> Result<Option<IpRange<Ipv4Net>>, String> {
        ip_ranges
            .map(|v| {
                let range: Result<IpRange<Ipv4Net>, _> = v
                    .iter()
                    .map(|s| s.parse::<Ipv4Net>().map_err(|_| format!("could not parse IP ranges for user {}", username)))
                    .collect();
                range
            })
            .transpose()
    }

    fn check_password(given_password: &str, actual_password: &Password) -> Result<(), ()> {
        match actual_password {
            Password::PlainPassword { password } => {
                if password == given_password {
                    Ok(())
                } else {
                    Err(())
                }
            }
            Password::Pbkdf2Password {
                pbkdf2_iter,
                pbkdf2_salt,
                pbkdf2_key,
            } => verify(PBKDF2_HMAC_SHA256, *pbkdf2_iter, pbkdf2_salt, given_password.as_bytes(), pbkdf2_key).map_err(|_| ()),
        }
    }

    fn ip_ok(creds: &libunftp::auth::Credentials, actual_creds: &UserCreds) -> bool {
        match &actual_creds.allowed_ip_ranges {
            Some(allowed) => match creds.source_ip {
                std::net::IpAddr::V4(ref ip) => allowed.contains(ip),
                _ => false,
            },
            None => true,
        }
    }
}

#[async_trait]
impl Authenticator<DefaultUser> for JsonFileAuthenticator {
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, creds: &libunftp::auth::Credentials) -> Result<DefaultUser, AuthenticationError> {
        let res = if let Some(actual_creds) = self.credentials_map.get(username) {
            match creds.password {
                None => Err(AuthenticationError::BadPassword),
                Some(ref given_password) => {
                    if Self::check_password(given_password, &actual_creds.password).is_ok() {
                        Self::ip_ok(creds, &actual_creds)
                            .then(|| Ok(DefaultUser {}))
                            .unwrap_or(Err(AuthenticationError::IpDisallowed))
                    } else {
                        Err(AuthenticationError::BadPassword)
                    }
                }
            }
        } else {
            Err(AuthenticationError::BadUser)
        };

        if res.is_err() {
            sleep(Duration::from_millis(1500)).await;
        }

        res
    }
}

mod test {
    #[tokio::test]
    async fn test_json_auth() {
        use super::*;

        let json: &str = r#"[
  {
    "username": "alice",
    "pbkdf2_salt": "dGhpc2lzYWJhZHNhbHQ=",
    "pbkdf2_key": "jZZ20ehafJPQPhUKsAAMjXS4wx9FSbzUgMn7HJqx4Hg=",
    "pbkdf2_iter": 500000
  },
  {
    "username": "bella",
    "pbkdf2_salt": "dGhpc2lzYWJhZHNhbHR0b28=",
    "pbkdf2_key": "C2kkRTybDzhkBGUkTn5Ys1LKPl8XINI46x74H4c9w8s=",
    "pbkdf2_iter": 500000
  },
  {
    "username": "carol",
    "password": "not so secure"
  },
  {
    "username": "dan",
    "password": "",
    "allowed_ip_ranges": ["127.0.0.1/8"]
  }
]"#;
        let json_authenticator = JsonFileAuthenticator::from_json(json).unwrap();
        assert_eq!(
            json_authenticator
                .authenticate("alice", &"this is the correct password for alice".into())
                .await
                .unwrap(),
            DefaultUser
        );
        assert_eq!(
            json_authenticator
                .authenticate("bella", &"this is the correct password for bella".into())
                .await
                .unwrap(),
            DefaultUser
        );
        assert_eq!(json_authenticator.authenticate("carol", &"not so secure".into()).await.unwrap(), DefaultUser);
        assert_eq!(json_authenticator.authenticate("dan", &"".into()).await.unwrap(), DefaultUser);
        match json_authenticator.authenticate("carol", &"this is the wrong password".into()).await {
            Err(AuthenticationError::BadPassword) => assert!(true),
            _ => assert!(false),
        }
        match json_authenticator.authenticate("bella", &"this is the wrong password".into()).await {
            Err(AuthenticationError::BadPassword) => assert!(true),
            _ => assert!(false),
        }
        match json_authenticator.authenticate("chuck", &"12345678".into()).await {
            Err(AuthenticationError::BadUser) => assert!(true),
            _ => assert!(false),
        }
    }
}
