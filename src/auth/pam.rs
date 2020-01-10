use crate::auth::*;

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
    fn authenticate(&self, username: &str, password: &str) -> Box<dyn Future<Output = Result<AnonymousUser, ()>> + Send> {
        let service = self.service.clone();
        let username = username.to_string();
        let password = password.to_string();

        let mut auth = match pam_auth::Authenticator::new(&service) {
            Some(auth) => auth,
            None => return Box::new(futures::future::err(())),
        };

        auth.set_credentials(&username, &password);
        let a = auth.authenticate().map(|_| AnonymousUser {}).map_err(|err| {
            debug!("RestError: {:?}", err);
        });

        Box::new(futures::future::ready(a))
    }
}
