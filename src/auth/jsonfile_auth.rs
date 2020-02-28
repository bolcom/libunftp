use crate::auth::*;

use futures::Future;

use std::io::Error;

use serde::Deserialize;
use serde_json;
use std::fs;

use std::time::{Duration, Instant};
use tokio::timer::Delay;

#[derive(Deserialize, Clone, Debug)]
struct Credentials {
    username: String,
    password: String,
}

/// [`Authenticator`] implementation that authenticates against a JSON file.
///
/// [`Authenticator`]: ../trait.Authenticator.html
///
/// Example credentials file format:
/// [
//   {
//     "username": "alice",
//     "password": "12345678"
//   },
//   {
//     "username": "bob",
//     "password": "secret"
//   }
// ]
#[derive(Clone, Debug)]
pub struct JsonFileAuthenticator {
    credentials_list: Vec<Credentials>,
}

impl JsonFileAuthenticator {
    /// Initialize a new [`JsonFileAuthenticator`] from file.
    pub fn new<T: Into<String>>(filename: T) -> Result<Self, Error> {
        let s = fs::read_to_string(filename.into())?;
        let credentials_list: Vec<Credentials> = serde_json::from_str(&s)?;
        Ok(JsonFileAuthenticator { credentials_list })
    }
}

impl Authenticator<AnonymousUser> for JsonFileAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Box<dyn Future<Item = AnonymousUser, Error = ()> + Send> {
        let username = _username.to_string();
        let password = _password.to_string();
        let credentials_list = self.credentials_list.clone();

        Box::new(
            futures::future::ok(())
                .and_then(move |_| {
                    for c in credentials_list.iter() {
                        if username == c.username {
                            if password == c.password {
                                info!("Successful login by user {}", username);
                                return Ok(AnonymousUser {});
                            } else {
                                warn!("Failed login for user {}: bad password", username);
                                return Err(());
                            }
                        }
                    }
                    warn!("Failed login for user \"{}\": unknown user", username);
                    Err(())
                })
                .or_else(|_| {
                    // punish the failed login with a 1500ms delay before returning the error
                    let when = Instant::now() + Duration::from_millis(1500);
                    Delay::new(when).map_err(|e| panic!("timer failed; err={:?}", e)).and_then(|_| Err(()))
                }),
        )
    }
}
