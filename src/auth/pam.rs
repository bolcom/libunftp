use crate::auth::Authenticator;

use futures::Future;

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: ../trait.Authenticator.html
/// [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module
pub struct PAMAuthenticator {
    service: String,
}

impl PAMAuthenticator {
    /// Initialize a new [`PAMAuthenticator`] for the given PAM service.
    pub fn new<S: Into<String>>(service: S) -> Self {
        let service = service.into();
        PAMAuthenticator { service }
    }
}

impl Authenticator<AnonymousUser> for PAMAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Box<dyn Future<Item = AnonymousUser, Error = ()> + Send> {
        let service = self.service.clone();
        let username = _username.to_string();
        let password = _password.to_string();

        Box::new(
            futures::future::ok(())
                .and_then(move |_| {
                    let mut auth = match pam_auth::Authenticator::new(&service) {
                        Some(auth) => auth,
                        None => return Err(()),
                    };

                    auth.set_credentials(&username, &password);
                    match auth.authenticate() {
                        Ok(()) => Ok(AnonymousUser {}),
                        Err(_) => Err(()),
                    }
                })
                .map_err(|err| {
                    debug!("RestError: {:?}", err);
                    ()
                }),
        )
    }
}

/// AnonymousUser
#[derive(Clone, Debug)]
pub struct AnonymousUser;
