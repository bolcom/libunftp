//! [`Authenticator`] implementation that authenticates against a JSON file.
//!
//! [`Authenticator`]: libunftp::auth::Authenticator

use async_trait::async_trait;
use libunftp::auth::{AuthenticationError, Authenticator, DefaultUser};
use ring::{
    digest::SHA256_OUTPUT_LEN,
    pbkdf2::{verify, PBKDF2_HMAC_SHA256},
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap},
    convert::TryInto,
    fs,
    num::NonZeroU32,
    path::Path,
    time::Duration,
};
use tokio::time::sleep;
use bytes::Bytes;
use valid::{
    Validate,
    constraint::Length,
}

#[derive(Deserialize, Clone, Debug)]
struct Credentials {
    username: String,
    pbkdf2_salt: String,
    pbkdf2_key: String,
    pbkdf2_iter: NonZeroU32,
}

/// [`Authenticator`](libunftp::auth::Authenticator) implementation that authenticates against JSON.
///
/// Example of using nettle-pbkdf2 with a generated 256 bit secure salt
///
/// Generate a secure salt:
/// salt=$(dd if=/dev/random bs=1 count=8)
///
/// Generate the base64 encoded PBKDF2 key, to be copied into the pbkdf2_key:
/// echo -n "mypassword" | nettle-pbkdf2 -i 5000 -l 32 --hex-salt $(echo -n $salt | xxd -p -c 80) --raw |openssl base64 -A
///
/// Convert the salt into base64 to be copied into the pbkdf2_salt:
/// echo -n $salt | openssl base64 -A
///
/// Verifies passwords against pbkdf2_key using the corresponding parameters form JSON.
/// Example credentials file format:
/// [
//   {
//     "username": "testuser1",
//     "pbkdf2_salt": "<<BASE_64_RANDOM_SALT>>",
//     "pbkdf2_key": "<<BASE_64_KDF>>",
//     "pbkdf2_iter": 500000
//   },
//   {
//     "username": "testuser2",
//     "pbkdf2_salt": "<<BASE_64_RANDOM_SALT>>",
//     "pbkdf2_key": "<<BASE_64_KDF>>",
//     "pbkdf2_iter": 500000
//   }
// ]

#[derive(Clone, Debug)]
pub struct JsonFileAuthenticator {
    db: HashMap<String, Password>,
}

#[derive(Clone, Debug)]
struct Password {
    pbkdf2_salt: Bytes,
    pbkdf2_key: Bytes,
    pbkdf2_iter: NonZeroU32,
}

impl JsonFileAuthenticator {
    /// Initialize a new [`JsonFileAuthenticator`] from file.
    pub fn from_file<P: AsRef<Path>>(filename: P) -> Result<Self, Box<dyn std::error::Error>> {
        let json: String = fs::read_to_string(filename)?;

        JsonFileAuthenticator::from_json(json)
    }

    /// Initialize a new [`JsonFileAuthenticator`] from json string.
    pub fn from_json<T: Into<String>>(json: T) -> Result<Self, Box<dyn std::error::Error>> {
        let db: Vec<Credentials> = serde_json::from_str::<Vec<Credentials>>(&json.into())?;
        let salts: BTreeSet<String> = db.iter().map(|credential| credential.pbkdf2_salt.clone()).collect();
        if db.len() != salts.len() {
            return Err(Box::new(AuthenticationError::new("The provided salts for the JsonFileAuthenticator must be unique.")));
        }
        Ok(JsonFileAuthenticator {
            db: db
                .into_iter()
                .map(|user_info| {
                    (
                        user_info.username.clone(),
                        Password {
                            pbkdf2_salt: base64::decode(user_info.pbkdf2_salt)
                                .expect("Could not base64 decode the salt")
                                .try_into()
                                .expect("Could not convert String to Bytes"),
                            pbkdf2_key: base64::decode(user_info.pbkdf2_key)
                                .expect("Could not decode base64")
                                .validate("pbkdf2_key", &Length::Max(SHA256_OUTPUT_LEN))
                                .result()
                                .unwrap_or_else({ let u = user_info.username.clone(); move |_| panic!("Key of user \"{}\" is too long", &u) })
                                .unwrap()
                                .try_into()
                                .expect("Could not convert to Bytes"),
                            pbkdf2_iter: user_info.pbkdf2_iter,
                        },
                    )
                })
                .into_iter()
                .collect(),
        })
    }
}

#[async_trait]
impl Authenticator<DefaultUser> for JsonFileAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, password: &str) -> Result<DefaultUser, AuthenticationError> {
        let db: HashMap<String, Password> = self.db.clone();

        if let Some(c) = db.get(username) {
            if let Ok(()) = verify(PBKDF2_HMAC_SHA256, c.pbkdf2_iter, &c.pbkdf2_salt, password.as_bytes(), &c.pbkdf2_key) {
                return Ok(DefaultUser);
            } else {
                sleep(Duration::from_millis(1500)).await;
                return Err(AuthenticationError::BadPassword);
            }
        } else {
            {
                sleep(Duration::from_millis(1500)).await;
                Err(AuthenticationError::BadUser)
            }
        }
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
  }
]"#;
        let json_authenticator = JsonFileAuthenticator::from_json(json).unwrap();
        assert_eq!(json_authenticator.authenticate("alice", "this is the correct password for alice").await.unwrap(), DefaultUser);
        assert_eq!(json_authenticator.authenticate("bella", "this is the correct password for bella").await.unwrap(), DefaultUser);
        match json_authenticator.authenticate("bella", "this is the wrong password").await {
            Err(AuthenticationError::BadUser) => assert!(true),
            _ => assert!(false),
        }
    }

    #[tokio::test]
    async fn test_salts_have_to_be_uniqe() {
        use super::*;

        let json: &str = r#"[
  {
    "username": "alice",
    "pbkdf2_salt": "bXlzYWx0",
    "pbkdf2_key": "b189PjHvYwkr23K6NfXehHpP/6GdFmtIgSTBbgVI7XQ=",
    "pbkdf2_iter": 500000
  },
  {
    "username": "bella",
    "pbkdf2_salt": "bXlzYWx0",
    "pbkdf2_key": "GtJ90iforiOhm0QTlutBZh/re0Tybd4zMj5KU4/AvtE=",
    "pbkdf2_iter": 500000
  }
]"#;
        assert!(JsonFileAuthenticator::from_json(json).is_err());
    }
}
