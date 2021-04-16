//! [`Authenticator`] implementation that authenticates against a JSON file.
//!
//! [`Authenticator`]: libunftp::auth::Authenticator

use async_trait::async_trait;
use libunftp::auth::{AuthenticationError, Authenticator, DefaultUser};
use ring::{
    digest::SHA512_OUTPUT_LEN,
    pbkdf2::{verify, PBKDF2_HMAC_SHA512},
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

#[derive(Deserialize, Clone, Debug)]
struct Credentials {
    username: String,
    pbkdf2_salt: String,
    pbkdf2_key: String,
    pbkdf2_iter: NonZeroU32,
}

/// [`Authenticator`](libunftp::auth::Authenticator) implementation that authenticates against a JSON.
///
/// Verifies passwords against pbkdf2_key using the corresponding parameters form JSON.
/// Example credentials file format:
/// [
//   {
//     "username": "testuser1",
//     "pbkdf2_salt": "testuser1.acme.bol.com",
//     "pbkdf2_key": "<<BASE_64_KDF>>",
//     "pbkdf2_iter": 500000
//   },
//   {
//     "username": "testuser2",
//     "pbkdf2_salt": "testuser2.acme.bol.com",
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
    pbkdf2_salt: String,
    pbkdf2_key: [u8; SHA512_OUTPUT_LEN],
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
            return Err(Box::new(std::io::Error::from(std::io::ErrorKind::InvalidData)));
        }
        Ok(JsonFileAuthenticator {
            db: db
                .into_iter()
                .map(|user_info| {
                    (
                        user_info.username,
                        Password {
                            pbkdf2_salt: user_info.pbkdf2_salt,
                            pbkdf2_key: base64::decode(user_info.pbkdf2_key)
                                .expect("Could not base64 decode the key")
                                .try_into()
                                .expect(format!("Could not convert Vec<u8> to [u8; {}]", SHA512_OUTPUT_LEN).as_str()),
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
            if let Ok(()) = verify(PBKDF2_HMAC_SHA512, c.pbkdf2_iter, c.pbkdf2_salt.as_bytes(), password.as_bytes(), &c.pbkdf2_key) {
                return Ok(DefaultUser);
            } else {
                return Err(AuthenticationError::BadUser);
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
    "pbkdf2_salt": "thisisabadsalt",
    "pbkdf2_key": "Egbi+LYfwn00V+HwFq146kmhoE4TYaqPFCA7mKkfzEpSZe2zMqXz/8LfA7HjYvXgiLzOuDij2wf50eKcWOcjYQ==",
    "pbkdf2_iter": 5000
  },
  {
    "username": "bella",
    "pbkdf2_salt": "thisisabadsalttoo",
    "pbkdf2_key": "9QSFDFRU80n1Jktu6s3Wo0XEArW3eQdw9zt4L9OBJjsGOYAsHfWqR4RKGwzve0Dih2M3Az+HHvKC9f43wYRRng==",
    "pbkdf2_iter": 5000
  }
]"#;
        let json_authenticator = JsonFileAuthenticator::from_json(json).unwrap();
        assert_eq!(json_authenticator.authenticate("alice", "not secret").await.unwrap(), DefaultUser);
        assert_eq!(json_authenticator.authenticate("bella", "also not secret").await.unwrap(), DefaultUser);
        match json_authenticator.authenticate("bella", "bad secret").await {
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
    "pbkdf2_salt": "salt",
    "pbkdf2_key": "Egbi+LYfwn00V+HwFq146kmhoE4TYaqPFCA7mKkfzEpSZe2zMqXz/8LfA7HjYvXgiLzOuDij2wf50eKcWOcjYQ==",
    "pbkdf2_iter": 5000
  },
  {
    "username": "bella",
    "pbkdf2_salt": "salt",
    "pbkdf2_key": "9QSFDFRU80n1Jktu6s3Wo0XEArW3eQdw9zt4L9OBJjsGOYAsHfWqR4RKGwzve0Dih2M3Az+HHvKC9f43wYRRng==",
    "pbkdf2_iter": 5000
  }
]"#;
        assert!(JsonFileAuthenticator::from_json(json).is_err());
    }
}
