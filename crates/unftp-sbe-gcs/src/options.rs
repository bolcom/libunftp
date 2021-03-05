//! Contains code pertaining to initialization options for the [`Cloud Storage Backend`](super::CloudStorage)

use core::fmt;
use std::{convert::TryFrom, path::PathBuf};
use yup_oauth2::ServiceAccountKey;

/// Used with [`CloudStorage::new`](super::CloudStorage::new()) to specify how the storage back-end
/// will authenticate with Google Cloud Storage.
#[derive(PartialEq, Clone)]
pub enum AuthMethod {
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
            AuthMethod::WorkloadIdentity(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "service account key not chosen as option")),
            AuthMethod::ServiceAccountKey(key) => {
                serde_json::from_slice(key).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("bad service account key: {}", e)))
            }
        }
    }
}

impl fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::WorkloadIdentity(None) => write!(f, "Workload Identity"),
            AuthMethod::WorkloadIdentity(Some(s)) => write!(f, "Workload Identity with service account {}", s),
            AuthMethod::ServiceAccountKey(_) => write!(f, "Service Account Key"),
        }
    }
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::WorkloadIdentity(None) => write!(f, "WorkloadIdentity(None)"),
            AuthMethod::WorkloadIdentity(Some(s)) => write!(f, "WorkloadIdentity(Some({}))", s),
            AuthMethod::ServiceAccountKey(_) => write!(f, "ServiceAccountKey(*******)"),
        }
    }
}
