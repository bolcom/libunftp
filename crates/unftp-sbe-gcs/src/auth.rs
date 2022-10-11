//! Contains code pertaining to authorization for the [`Cloud Storage Backend`](super::CloudStorage)

use async_trait::async_trait;
use core::fmt;
use libunftp::storage::Error;
use std::{convert::TryFrom, path::PathBuf};
use time::OffsetDateTime;
use yup_oauth2::ServiceAccountKey;

/// Token represents an OAuth2 access token
pub struct Token {
    /// access_token is the value of the access token
    pub access_token: String,

    /// expires_at is the time when this token will expire. A None value means that the token is to
    /// be considered expired.
    pub expires_at: Option<OffsetDateTime>,
}

#[async_trait]
/// TokenProvider defines the contract for getting OAuth2 tokens.
pub trait TokenProvider: Sync + Send + TokenProviderClone + std::fmt::Debug {
    /// get_token gets the OAuth2 access token from an auth server.
    async fn get_token(&self) -> Result<Token, Error>;
}

/// TokenProviderClone allows a boxed TokenProvider to be cloned.
pub trait TokenProviderClone {
    /// clone_box clones the boxed TokenProvider.
    fn clone_box(&self) -> Box<dyn TokenProvider>;
}

impl<T> TokenProviderClone for T
where
    T: 'static + TokenProvider + Clone,
{
    fn clone_box(&self) -> Box<dyn TokenProvider> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn TokenProvider> {
    fn clone(&self) -> Box<dyn TokenProvider> {
        self.clone_box()
    }
}

#[derive(Clone, Debug)]
pub(super) struct NoopTokenProvider;

#[async_trait]
impl TokenProvider for NoopTokenProvider {
    async fn get_token(&self) -> Result<Token, Error> {
        Ok(Token {
            access_token: "unftp_test".to_string(),
            expires_at: None,
        })
    }
}

/// Used with [`CloudStorage::new`](super::CloudStorage::new()) to specify how the storage back-end
/// will authenticate with Google Cloud Storage.
#[derive(PartialEq, Eq, Clone)]
pub enum AuthMethod {
    /// Used for testing purposes only
    None,
    /// Authenticate using a private service account key
    ServiceAccountKey(Vec<u8>),
    /// Authenticate using GCE [Workload Identity](https://cloud.google.com/blog/products/containers-kubernetes/introducing-workload-identity-better-authentication-for-your-gke-applications)
    WorkloadIdentity(Option<String>),
}

impl From<Vec<u8>> for AuthMethod {
    fn from(service_account_key: Vec<u8>) -> Self {
        if service_account_key.is_empty() {
            return AuthMethod::WorkloadIdentity(None);
        }
        AuthMethod::ServiceAccountKey(service_account_key)
    }
}

impl TryFrom<PathBuf> for AuthMethod {
    type Error = std::io::Error;

    fn try_from(service_account_key_file: PathBuf) -> Result<Self, Self::Error> {
        match std::fs::read(service_account_key_file) {
            Err(e) => Err(e),
            Ok(v) => Ok(v.into()),
        }
    }
}

impl TryFrom<Option<PathBuf>> for AuthMethod {
    type Error = std::io::Error;

    fn try_from(service_account_key_file: Option<PathBuf>) -> Result<Self, Self::Error> {
        match service_account_key_file {
            Some(p) => AuthMethod::try_from(p),
            None => Ok(AuthMethod::WorkloadIdentity(None)),
        }
    }
}

impl AuthMethod {
    pub(super) fn to_service_account_key(&self) -> std::io::Result<ServiceAccountKey> {
        match self {
            AuthMethod::WorkloadIdentity(_) | AuthMethod::None => {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "service account key not chosen as option"))
            }
            AuthMethod::ServiceAccountKey(key) => {
                serde_json::from_slice(key).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("bad service account key: {}", e)))
            }
        }
    }
}

impl fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::WorkloadIdentity(None) => {
                write!(f, "Workload Identity")
            }
            AuthMethod::WorkloadIdentity(Some(s)) => {
                write!(f, "Workload Identity with service account {}", s)
            }
            AuthMethod::ServiceAccountKey(_) => {
                write!(f, "Service Account Key")
            }
            AuthMethod::None => write!(f, "None"),
        }
    }
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::WorkloadIdentity(None) => {
                write!(f, "WorkloadIdentity(None)")
            }
            AuthMethod::WorkloadIdentity(Some(s)) => {
                write!(f, "WorkloadIdentity(Some({}))", s)
            }
            AuthMethod::ServiceAccountKey(_) => {
                write!(f, "ServiceAccountKey(*******)")
            }
            AuthMethod::None => write!(f, "None"),
        }
    }
}
