//! This crate implements a [libunftp](https://docs.rs/libunftp/latest/libunftp) `Authenticator`
//! that authenticates against credentials in a JSON format.
//!
//! It supports both plaintext as well as [PBKDF2](https://tools.ietf.org/html/rfc2898#section-5.2)
//! encoded passwords.
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
//!
//! In this example we show two ways to generate the PBKDF2. First we show how to use the common tool [nettle-pbkdf2](http://www.lysator.liu.se/~nisse/nettle/) directly.
//!
//! Generate a secure salt:
//! ```sh
//! salt=$(dd if=/dev/random bs=1 count=8)
//! ```
//!
//! Generate the base64 encoded PBKDF2 key, to be copied into the `pbkdf2_key` field of the JSON structure.
//!
//! When using `nettle` directly, make sure not to exceed the output length of the digest algorithm (256 bit, 32 bytes in our case):
//! ```sh
//! echo -n "mypassword" | nettle-pbkdf2 -i 500000 -l 32 --hex-salt $(echo -n $salt | xxd -p -c 80) --raw |openssl base64 -A
//! ```
//!
//! Convert the salt into base64 to be copied into the `pbkdf2_salt` field of the JSON structure:
//! ```sh
//! echo -n $salt | openssl base64 -A
//! ```
//!
//! Alternatively to using `nettle` directly, you may use our convenient docker image: bolcom/unftp-key-generator
//!
//! ```sh
//! docker run -ti bolcom/unftp-key-generator -h
//! ```
//!
//! Running it without options, will generate a PBKDF2 key and a random salt from a given password.
//! If no password is entered, a secure password will be generated with default settings for the password complexity and number of iterations.
//!
//! Now write these to the JSON file, as seen below.
//! If you use our unftp-key-generator, you can use the `-u` switch, to generate the JSON output directly.
//! Otherwise, make sure that `pbkdf2_iter` in the example below, matches the iterations (`-i`) used with `nettle-pbkdf2`.
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
//! # Using it with libunftp
//!
//! Use [JsonFileAuthenticator::from_file](crate::JsonFileAuthenticator::from_file) to load the JSON structure directly from a file.
//! See the example `examples/jsonfile_auth.rs`.
//!
//! Alternatively use another source for your JSON credentials, and use [JsonFileAuthenticator::from_json](crate::JsonFileAuthenticator::from_json) instead.
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
//! # Per user certificate validation
//!
//! The JSON authenticator can also check that the CN of a client certificate matches a certain
//! string or substring. Furthermore, password-less; certificate only; authentication can be configured
//! per user when libunftp is configured to use TLS and specifically also configured to request or
//! require a client certificate through the [Server.ftps_client_auth](https://docs.rs/libunftp/0.17.4/libunftp/struct.Server.html#method.ftps_client_auth)
//! method. For this to work correctly a trust store with the root certificate also needs to be configured
//! with [Server.ftps_trust_store](https://docs.rs/libunftp/0.17.4/libunftp/struct.Server.html#method.ftps_trust_store).
//!
//! Given this example configuration:
//!
//! ```json
//! [
//!   {
//!    "username": "eve",
//!    "pbkdf2_salt": "dGhpc2lzYWJhZHNhbHR0b28=",
//!    "pbkdf2_key": "C2kkRTybDzhkBGUkTn5Ys1LKPl8XINI46x74H4c9w8s=",
//!    "pbkdf2_iter": 500000,
//!    "client_cert": {
//!      "allowed_cn": "i.am.trusted"
//!    }
//!  },
//!  {
//!    "username": "freddie",
//!    "client_cert": {}
//!  },
//!  {
//!    "username": "santa",
//!    "password": "clara",
//!    "client_cert": {}
//!  }
//! ]
//! ```
//!
//! we can see that Eve needs to present a valid client certificate with a CN matching "i.am.trusted"
//! and then also needs to provide the correct password. Freddie just needs to present a valid
//! certificate that is signed by a certificate in the trust store. No password is required for
//! him when logging in. Santa needs to provide a valid certificate and password but the CN can
//! be anything.
//!

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use flate2::read::GzDecoder;
use ipnet::Ipv4Net;
use iprange::IpRange;
use ring::{
    digest::SHA256_OUTPUT_LEN,
    pbkdf2::{PBKDF2_HMAC_SHA256, verify},
};
use serde::Deserialize;
use std::{collections::HashMap, fs, io::prelude::*, num::NonZeroU32, path::Path, time::Duration};
use tokio::time::sleep;
use unftp_core::auth::{AuthenticationError, Authenticator, Principal};
use valid::{Validate, constraint::Length};

#[derive(Deserialize, Clone, Debug)]
struct ClientCertCredential {
    allowed_cn: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum Credentials {
    Pbkdf2 {
        username: String,
        pbkdf2_salt: String,
        pbkdf2_key: String,
        pbkdf2_iter: NonZeroU32,
        client_cert: Option<ClientCertCredential>,
        allowed_ip_ranges: Option<Vec<String>>,
    },
    Plaintext {
        username: String,
        password: Option<String>,
        client_cert: Option<ClientCertCredential>,
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
        password: Option<String>,
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
    pub client_cert: Option<ClientCertCredential>,
    pub allowed_ip_ranges: Option<IpRange<Ipv4Net>>,
}

impl JsonFileAuthenticator {
    /// Initialize a new [`JsonFileAuthenticator`] from file.
    pub fn from_file<P: AsRef<Path>>(filename: P) -> Result<Self, Box<dyn std::error::Error>> {
        let mut f = fs::File::open(&filename)?;

        // The credentials file can be plaintext, gzipped, or gzipped+base64-encoded
        // The gzip-base64 format is useful for overcoming configmap size limits in Kubernetes
        let mut magic: [u8; 4] = [0; 4];
        let n = f.read(&mut magic[..])?;
        let is_gz = n > 2 && magic[0] == 0x1F && magic[1] == 0x8B && magic[2] == 0x8;
        // the 3 magic bytes translate to "H4sI" in base64
        let is_base64gz = n > 3 && magic[0] == b'H' && magic[1] == b'4' && magic[2] == b's' && magic[3] == b'I';

        f.rewind()?;
        let json: String = if is_gz | is_base64gz {
            let mut gzdata: Vec<u8> = Vec::new();
            if is_base64gz {
                let mut b = Vec::new();
                f.read_to_end(&mut b)?;
                b.retain(|&x| x != b'\n' && x != b'\r');
                gzdata = base64::engine::general_purpose::STANDARD.decode(b)?;
            } else {
                f.read_to_end(&mut gzdata)?;
            }
            let mut d = GzDecoder::new(&gzdata[..]);
            let mut s = String::new();
            d.read_to_string(&mut s)?;
            s
        } else {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            s
        };

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
                client_cert,
                allowed_ip_ranges: ip_ranges,
            } => (
                username.clone(),
                UserCreds {
                    password: Password::PlainPassword { password },
                    client_cert,
                    allowed_ip_ranges: Self::parse_ip_range(username, ip_ranges)?,
                },
            ),
            Credentials::Pbkdf2 {
                username,
                pbkdf2_salt,
                pbkdf2_key,
                pbkdf2_iter,
                client_cert,
                allowed_ip_ranges: ip_ranges,
            } => (
                username.clone(),
                UserCreds {
                    password: Password::Pbkdf2Password {
                        pbkdf2_salt: base64::engine::general_purpose::STANDARD
                            .decode(pbkdf2_salt)
                            .map_err(|_| "Could not base64 decode the salt")?
                            .into(),
                        pbkdf2_key: base64::engine::general_purpose::STANDARD
                            .decode(pbkdf2_key)
                            .map_err(|_| "Could not decode base64")?
                            .validate("pbkdf2_key", &Length::Max(SHA256_OUTPUT_LEN))
                            .result()
                            .map_err(|_| format!("Key of user \"{}\" is too long", username))?
                            .unwrap() // Safe to use given Validated's API
                            .into(),
                        pbkdf2_iter,
                    },
                    client_cert,
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
                if let Some(pwd) = password {
                    if pwd == given_password { Ok(()) } else { Err(()) }
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

    fn ip_ok(creds: &unftp_core::auth::Credentials, actual_creds: &UserCreds) -> bool {
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
impl Authenticator for JsonFileAuthenticator {
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, creds: &unftp_core::auth::Credentials) -> Result<Principal, AuthenticationError> {
        let res = if let Some(actual_creds) = self.credentials_map.get(username) {
            let client_cert = &actual_creds.client_cert;
            let certificate = &creds.certificate_chain.as_ref().and_then(|x| x.first());

            let ip_check_result = if !Self::ip_ok(creds, actual_creds) {
                Err(AuthenticationError::IpDisallowed)
            } else {
                Ok(Principal {
                    username: username.to_string(),
                })
            };

            let cn_check_result = match (&client_cert, certificate) {
                // If client_cert is Some, it has an allowed_cn
                // Option, if it is set, the client cert is checked,
                // otherwise any trusted client cert will be accepted
                (Some(client_cert), Some(cert)) => match (&client_cert.allowed_cn, cert) {
                    (Some(cn), cert) => match cert.verify_cn(cn) {
                        Ok(is_authorized) => {
                            if is_authorized {
                                Some(Ok(Principal {
                                    username: username.to_string(),
                                }))
                            } else {
                                Some(Err(AuthenticationError::CnDisallowed))
                            }
                        }
                        Err(e) => Some(Err(AuthenticationError::with_source("verify_cn", e))),
                    },
                    (None, _) => Some(Ok(Principal {
                        username: username.to_string(),
                    })),
                },
                (Some(_), None) => Some(Err(AuthenticationError::CnDisallowed)),
                _ => None,
            };

            let pass_check_result = match &creds.password {
                Some(given_password) => {
                    if Self::check_password(given_password, &actual_creds.password).is_ok() {
                        Some(Ok(Principal {
                            username: username.to_string(),
                        }))
                    } else {
                        Some(Err(AuthenticationError::BadPassword))
                    }
                }
                None => None,
            };

            // the ip_check_result is returned at the end if all the other credentials are good.
            // because from unauthorized sources, we want to know whether they posses valid credentials somehow
            // but for logging purposes it would be better if we simply logged all of the results instead
            match (pass_check_result, cn_check_result, ip_check_result) {
                (None, None, _) => Err(AuthenticationError::BadPassword), // At least a password or client cert check is required
                (Some(pass_res), None, ip_res) => {
                    if pass_res.is_ok() {
                        ip_res
                    } else {
                        pass_res
                    }
                }
                (None, Some(cn_res), ip_res) => {
                    if cn_res.is_ok() {
                        ip_res
                    } else {
                        cn_res
                    }
                }
                (Some(pass_res), Some(cn_res), ip_res) => match (pass_res, cn_res) {
                    (Ok(_), Ok(_)) => ip_res,
                    (Ok(_), Err(e)) => Err(e),
                    (Err(e), Ok(_)) => Err(e),
                    (Err(e), Err(_)) => Err(e), // AuthenticationError::BadPassword returned also if both password and CN are wrong
                },
            }
        } else {
            Err(AuthenticationError::BadUser)
        };

        if res.is_err() {
            sleep(Duration::from_millis(1500)).await;
        }

        res
    }

    /// Tells whether its OK to not ask for a password when a valid client cert
    /// was presented.
    ///
    /// For this JSON authenticator, if a certificate object is given
    /// (optionally matched against client certificate of a specific
    /// user during authentication), the user can omit the password as
    /// a way to indicate that the user + client cert is sufficient
    /// for authentication. If the password is given, then both are
    /// required.
    async fn cert_auth_sufficient(&self, username: &str) -> bool {
        if let Some(actual_creds) = self.credentials_map.get(username)
            && let Password::PlainPassword { password: None } = &actual_creds.password
        {
            return actual_creds.client_cert.is_some();
        }
        false
    }

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

mod test {
    #[allow(unused_imports)]
    use unftp_core::auth::ChannelEncryptionState;
    #[allow(unused_imports)]
    use unftp_core::auth::ClientCert;

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
                .unwrap()
                .username,
            "alice"
        );
        assert_eq!(
            json_authenticator
                .authenticate("bella", &"this is the correct password for bella".into())
                .await
                .unwrap()
                .username,
            "bella"
        );
        assert_eq!(
            json_authenticator.authenticate("carol", &"not so secure".into()).await.unwrap().username,
            "carol"
        );
        assert_eq!(json_authenticator.authenticate("dan", &"".into()).await.unwrap().username, "dan");
        assert!(matches!(
            json_authenticator.authenticate("carol", &"this is the wrong password".into()).await,
            Err(AuthenticationError::BadPassword)
        ));
        assert!(matches!(
            json_authenticator.authenticate("bella", &"this is the wrong password".into()).await,
            Err(AuthenticationError::BadPassword)
        ));
        assert!(matches!(
            json_authenticator.authenticate("chuck", &"12345678".into()).await,
            Err(AuthenticationError::BadUser)
        ));

        assert_eq!(
            json_authenticator
                .authenticate(
                    "dan",
                    &unftp_core::auth::Credentials {
                        certificate_chain: None,
                        password: Some("".into()),
                        source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                        command_channel_security: ChannelEncryptionState::Plaintext,
                    },
                )
                .await
                .unwrap()
                .username,
            "dan"
        );

        match json_authenticator
            .authenticate(
                "dan",
                &unftp_core::auth::Credentials {
                    certificate_chain: None,
                    password: Some("".into()),
                    source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(128, 0, 0, 1)),
                    command_channel_security: ChannelEncryptionState::Plaintext,
                },
            )
            .await
        {
            Err(AuthenticationError::IpDisallowed) => (),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn test_json_cert_sufficient() {
        use super::*;

        let json: &str = r#"[
  {
    "username": "alice",
    "password": "has a password"
  },
  {
    "username": "bob",
    "client_cert": {
      "allowed_cn": "my.cert.is.everything"
    }
  },
  {
    "username": "carol",
    "password": "This is ultimate security.",
    "client_cert": {
      "allowed_cn": "i.am.trusted"
    }
  },
  {
    "username": "dan",
    "pbkdf2_salt": "dGhpc2lzYWJhZHNhbHQ=",
    "pbkdf2_key": "jZZ20ehafJPQPhUKsAAMjXS4wx9FSbzUgMn7HJqx4Hg=",
    "pbkdf2_iter": 500000
  },
  {
    "username": "eve",
    "pbkdf2_salt": "dGhpc2lzYWJhZHNhbHR0b28=",
    "pbkdf2_key": "C2kkRTybDzhkBGUkTn5Ys1LKPl8XINI46x74H4c9w8s=",
    "pbkdf2_iter": 500000,
    "client_cert": {
      "allowed_cn": "i.am.trusted"
    }
  },
  {
    "username": "freddie",
    "client_cert": {}
  },
  {
    "username": "santa",
    "password": "clara",
    "client_cert": {}
  }  
]"#;
        let json_authenticator = JsonFileAuthenticator::from_json(json).unwrap();
        assert!(!json_authenticator.cert_auth_sufficient("alice").await);
        assert!(json_authenticator.cert_auth_sufficient("bob").await);
        assert!(!json_authenticator.cert_auth_sufficient("carol").await);
        assert!(!json_authenticator.cert_auth_sufficient("dan").await);
        assert!(!json_authenticator.cert_auth_sufficient("eve").await);
        assert!(json_authenticator.cert_auth_sufficient("freddie").await);
        assert!(!json_authenticator.cert_auth_sufficient("santa").await);
    }

    #[tokio::test]
    async fn test_json_cert_authenticate() {
        use super::*;

        // DER formatted certificate: subject= /CN=unftp-client.mysite.com/O=mysite.com/C=NL
        let cert: &[u8] = &[
            0x30, 0x82, 0x03, 0x1f, 0x30, 0x82, 0x02, 0x07, 0xa0, 0x03, 0x02, 0x01, 0x02, 0x02, 0x09, 0x00, 0xc3, 0x3d, 0x48, 0x52, 0x68, 0x7e, 0x06, 0x83,
            0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b, 0x05, 0x00, 0x30, 0x40, 0x31, 0x1c, 0x30, 0x1a, 0x06, 0x03, 0x55,
            0x04, 0x03, 0x0c, 0x13, 0x75, 0x6e, 0x66, 0x74, 0x70, 0x2d, 0x63, 0x61, 0x2e, 0x6d, 0x79, 0x73, 0x69, 0x74, 0x65, 0x2e, 0x63, 0x6f, 0x6d, 0x31,
            0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x0a, 0x0c, 0x0a, 0x6d, 0x79, 0x73, 0x69, 0x74, 0x65, 0x2e, 0x63, 0x6f, 0x6d, 0x31, 0x0b, 0x30, 0x09,
            0x06, 0x03, 0x55, 0x04, 0x06, 0x13, 0x02, 0x4e, 0x4c, 0x30, 0x1e, 0x17, 0x0d, 0x32, 0x31, 0x30, 0x36, 0x32, 0x35, 0x31, 0x32, 0x30, 0x38, 0x30,
            0x38, 0x5a, 0x17, 0x0d, 0x32, 0x34, 0x30, 0x34, 0x31, 0x34, 0x31, 0x32, 0x30, 0x38, 0x30, 0x38, 0x5a, 0x30, 0x44, 0x31, 0x20, 0x30, 0x1e, 0x06,
            0x03, 0x55, 0x04, 0x03, 0x0c, 0x17, 0x75, 0x6e, 0x66, 0x74, 0x70, 0x2d, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x2e, 0x6d, 0x79, 0x73, 0x69, 0x74,
            0x65, 0x2e, 0x63, 0x6f, 0x6d, 0x31, 0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x0a, 0x0c, 0x0a, 0x6d, 0x79, 0x73, 0x69, 0x74, 0x65, 0x2e, 0x63,
            0x6f, 0x6d, 0x31, 0x0b, 0x30, 0x09, 0x06, 0x03, 0x55, 0x04, 0x06, 0x13, 0x02, 0x4e, 0x4c, 0x30, 0x82, 0x01, 0x22, 0x30, 0x0d, 0x06, 0x09, 0x2a,
            0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00, 0x03, 0x82, 0x01, 0x0f, 0x00, 0x30, 0x82, 0x01, 0x0a, 0x02, 0x82, 0x01, 0x01, 0x00,
            0xec, 0xdf, 0x85, 0x48, 0xf4, 0x20, 0xdd, 0x52, 0x0b, 0x9c, 0x08, 0x6a, 0x78, 0x0f, 0x16, 0x16, 0x8b, 0x11, 0x79, 0xef, 0x32, 0xb6, 0x55, 0x90,
            0x50, 0x31, 0x09, 0xf6, 0x1a, 0x99, 0xff, 0xa2, 0x51, 0x0f, 0x74, 0x2b, 0x80, 0xeb, 0x69, 0x8e, 0x42, 0x53, 0x54, 0x7d, 0xf0, 0x13, 0x92, 0x2d,
            0x86, 0xda, 0x3b, 0x7d, 0x2b, 0x19, 0x15, 0x3a, 0xeb, 0xb0, 0xd8, 0x33, 0xb4, 0x4c, 0xb0, 0x4e, 0x63, 0x32, 0x35, 0x8e, 0x30, 0xc9, 0xfe, 0xaf,
            0xcc, 0xc7, 0xa6, 0xdc, 0xbf, 0x83, 0x16, 0x6f, 0xdc, 0xc5, 0xdf, 0x10, 0x24, 0x45, 0xb0, 0x7c, 0x5b, 0x36, 0xc7, 0xcd, 0xf7, 0x5b, 0x1e, 0x9f,
            0xae, 0x80, 0xd8, 0x0e, 0x27, 0x0f, 0xb6, 0x04, 0x16, 0xa5, 0x4b, 0x58, 0x4c, 0xd5, 0x25, 0x1b, 0x99, 0x48, 0xd4, 0x02, 0x85, 0x25, 0x54, 0x31,
            0x2b, 0x77, 0x4d, 0xe9, 0x81, 0xbe, 0x81, 0x32, 0xee, 0x16, 0x59, 0x21, 0x82, 0x8c, 0x7d, 0x9f, 0xca, 0x93, 0xe4, 0x93, 0xb8, 0x2f, 0x0f, 0x16,
            0xa6, 0x43, 0x3e, 0xa6, 0x4f, 0xe0, 0xbd, 0xd5, 0x30, 0x05, 0x8e, 0xe1, 0x85, 0x12, 0xee, 0xbe, 0xa0, 0x1a, 0xa0, 0x63, 0x16, 0x3c, 0xf7, 0x73,
            0xe1, 0xe6, 0x76, 0xe5, 0x98, 0x82, 0x59, 0x88, 0xe4, 0xa4, 0xe2, 0xf9, 0xc7, 0xb8, 0x21, 0x4c, 0x3f, 0x9f, 0xeb, 0x06, 0x13, 0xf8, 0x67, 0x45,
            0x4e, 0xf0, 0xf8, 0x07, 0x59, 0x1f, 0x9d, 0x52, 0xb9, 0x19, 0xdb, 0x0e, 0x36, 0x92, 0x39, 0x85, 0xa5, 0x18, 0x30, 0x9f, 0x6b, 0x39, 0x9c, 0xba,
            0x09, 0xf0, 0xc5, 0xfc, 0x21, 0xf0, 0x27, 0xf9, 0x97, 0x45, 0x96, 0x38, 0x25, 0x56, 0x59, 0x18, 0x9c, 0x99, 0x75, 0x0a, 0x86, 0xb8, 0xc1, 0xb6,
            0x2c, 0xbe, 0x53, 0x4a, 0xe8, 0xd2, 0x8a, 0xf8, 0x47, 0xc3, 0x71, 0x60, 0x28, 0x88, 0xe1, 0x13, 0x02, 0x03, 0x01, 0x00, 0x01, 0xa3, 0x18, 0x30,
            0x16, 0x30, 0x14, 0x06, 0x03, 0x55, 0x1d, 0x11, 0x04, 0x0d, 0x30, 0x0b, 0x82, 0x09, 0x6c, 0x6f, 0x63, 0x61, 0x6c, 0x68, 0x6f, 0x73, 0x74, 0x30,
            0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b, 0x05, 0x00, 0x03, 0x82, 0x01, 0x01, 0x00, 0xb4, 0x57, 0x68, 0x3e, 0x4b,
            0x9b, 0x89, 0xbd, 0x60, 0x2b, 0xa7, 0x02, 0x04, 0xbf, 0xff, 0x56, 0xd5, 0xce, 0x1c, 0x20, 0x7f, 0x92, 0xa9, 0xd3, 0xf3, 0x8b, 0xfd, 0x8c, 0x41,
            0x19, 0xb5, 0xe5, 0x01, 0x0a, 0x5f, 0x2f, 0x86, 0x0b, 0x26, 0x71, 0x89, 0x7b, 0x0f, 0x2c, 0x1b, 0x54, 0xc9, 0x3a, 0xf4, 0x37, 0xdf, 0x52, 0x7d,
            0x87, 0x30, 0x49, 0xbf, 0x7c, 0x84, 0x46, 0x3c, 0x21, 0xbe, 0x99, 0x8f, 0x69, 0x56, 0x8c, 0x5f, 0x7c, 0xb0, 0xe9, 0xdc, 0xbd, 0xfa, 0xbe, 0x26,
            0xb6, 0xfa, 0xa5, 0xdd, 0x9b, 0x41, 0xe9, 0x2c, 0xd2, 0x21, 0x42, 0xe7, 0x67, 0xcc, 0x01, 0xda, 0x7a, 0xb7, 0x84, 0xa7, 0x83, 0x91, 0x37, 0x43,
            0x04, 0x3e, 0xde, 0x41, 0xba, 0x7d, 0xa3, 0x5c, 0xc0, 0x6f, 0x8c, 0x2c, 0x1c, 0xa8, 0x86, 0xa7, 0x38, 0xa4, 0x1f, 0x58, 0x7d, 0xb2, 0xf7, 0xc8,
            0xe2, 0x3c, 0x10, 0xd9, 0x69, 0x4b, 0xef, 0x3d, 0x47, 0x39, 0xf8, 0x3e, 0x87, 0x67, 0x7e, 0xfc, 0x43, 0xbb, 0x01, 0x7c, 0xa2, 0x26, 0xb9, 0xb1,
            0x3c, 0x1d, 0xd4, 0xbe, 0xa0, 0x02, 0x0d, 0x10, 0x62, 0xd9, 0xe3, 0x7f, 0x90, 0x30, 0x89, 0x64, 0x37, 0x90, 0xcd, 0x34, 0xd4, 0x03, 0x9f, 0x96,
            0x80, 0xb1, 0xaa, 0x93, 0x59, 0x23, 0xd7, 0xad, 0x3e, 0x13, 0x76, 0x02, 0x1f, 0xd2, 0xa6, 0x8b, 0x44, 0x26, 0x8f, 0x1d, 0xf8, 0x60, 0xba, 0xc5,
            0x52, 0x31, 0x26, 0x64, 0xca, 0x7e, 0x3f, 0xe9, 0xba, 0x72, 0xdc, 0x80, 0xfd, 0x4b, 0x10, 0x66, 0x5d, 0x85, 0xd3, 0xa3, 0x2b, 0xe6, 0x73, 0x4a,
            0xcf, 0xba, 0xe0, 0x48, 0x4f, 0x00, 0xed, 0xaa, 0xb3, 0x75, 0xe8, 0xbc, 0xf3, 0xba, 0xb7, 0x4d, 0x59, 0x17, 0xde, 0xb5, 0x2c, 0x8d, 0x9a, 0x88,
            0x34, 0x02, 0x19, 0x9c, 0x22, 0x56, 0x26, 0x3f, 0x3a, 0x6f, 0x0f,
        ];

        let json: &str = r#"[
  {
    "username": "alice",
    "password": "has a password",
    "client_cert": {
      "allowed_cn": "unftp-client.mysite.com"
    }
  },
  {
    "username": "bob",
    "client_cert": {
      "allowed_cn": "unftp-client.mysite.com"
    }
  },
  {
    "username": "carol",
    "client_cert": {
      "allowed_cn": "unftp-other-client.mysite.com"
    }
  },
  {
    "username": "dean",
    "client_cert": {}
  }
]"#;
        let json_authenticator = JsonFileAuthenticator::from_json(json).unwrap();
        let client_cert: Vec<u8> = cert.to_vec();

        // correct certificate and password combo authenticates successfully
        assert_eq!(
            json_authenticator
                .authenticate(
                    "alice",
                    &unftp_core::auth::Credentials {
                        certificate_chain: Some(vec![ClientCert(client_cert.clone())]),
                        password: Some("has a password".into()),
                        source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                        command_channel_security: ChannelEncryptionState::Plaintext,
                    },
                )
                .await
                .unwrap()
                .username,
            "alice"
        );

        // correct password but missing certificate fails
        match json_authenticator
            .authenticate(
                "alice",
                &unftp_core::auth::Credentials {
                    certificate_chain: None,
                    password: Some("has a password".into()),
                    source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                    command_channel_security: ChannelEncryptionState::Plaintext,
                },
            )
            .await
        {
            Err(AuthenticationError::CnDisallowed) => (),
            _ => panic!(),
        }

        // correct certificate and no password needed according to json file authenticates successfully
        assert_eq!(
            json_authenticator
                .authenticate(
                    "bob",
                    &unftp_core::auth::Credentials {
                        certificate_chain: Some(vec![ClientCert(client_cert.clone())]),
                        password: None,
                        source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                        command_channel_security: ChannelEncryptionState::Plaintext,
                    },
                )
                .await
                .unwrap()
                .username,
            "bob"
        );

        // certificate with incorrect CN and no password needed according to json file fails to authenticate
        match json_authenticator
            .authenticate(
                "carol",
                &unftp_core::auth::Credentials {
                    certificate_chain: Some(vec![ClientCert(client_cert.clone())]),
                    password: None,
                    source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                    command_channel_security: ChannelEncryptionState::Plaintext,
                },
            )
            .await
        {
            Err(AuthenticationError::CnDisallowed) => (),
            _ => panic!(),
        }

        // any trusted certificate without password according to json file authenticates successfully
        assert_eq!(
            json_authenticator
                .authenticate(
                    "dean",
                    &unftp_core::auth::Credentials {
                        certificate_chain: Some(vec![ClientCert(client_cert.clone())]),
                        password: None,
                        source_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                        command_channel_security: ChannelEncryptionState::Plaintext,
                    },
                )
                .await
                .unwrap()
                .username,
            "dean"
        );
    }
}
