use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

/// An event pertaining to a client's login and logout actions in order to allow detection of the
/// presence of a client. Instances of these will be passed to an [`PresenceListener`](crate::notification::PresenceListener).
/// To identify the corresponding user or session see the [`EventMeta`](crate::notification::EventMeta) struct.
#[derive(Debug, Clone)]
pub enum PresenceEvent {
    /// The user logged in successfully
    LoggedIn,
    /// The user logged out
    LoggedOut,
}

/// An event signalling a change in data on the storage back-end. To identify the corresponding user
/// or session see the [`EventMeta`](crate::notification::EventMeta) struct.
///
/// Paths are in the FTP namespace, rooted at the user's FTP root. Relative command arguments
/// include the working directory used for the operation: for example, `STOR photo.jpg` in
/// `/uploads` produces a [`DataEvent::Put`] with path `/uploads/photo.jpg`. At the FTP root,
/// the same command produces `/photo.jpg`. Absolute command arguments are not prefixed with
/// the working directory.
///
/// These are the paths supplied to the storage backend, not host filesystem paths or object
/// URLs. They are not canonicalized: parent-directory components such as `..` may remain,
/// and symlinks are not resolved. Path separators follow the server platform's path handling.
#[derive(Debug, Clone)]
pub enum DataEvent {
    /// A RETR command finished successfully
    Got {
        /// The path to the file that was obtained
        path: String,

        /// The amount of bytes transferred to the client
        bytes: u64,
    },
    /// A STOR or APPE command finished successfully
    Put {
        /// The path to the file that was written
        path: String,

        /// The amount of bytes stored
        bytes: u64,
    },
    /// A DEL command finished successfully
    Deleted {
        /// The path to the file that was deleted.
        path: String,
    },
    /// A MKD command finished successfully
    MadeDir {
        /// The path to the directory that was created
        path: String,
    },
    /// A RMD command finished successfully
    RemovedDir {
        /// The path to the directory that was removed
        path: String,
    },
    /// A RNFR & RNTO command sequence finished successfully. This can be for a file or a directory.
    Renamed {
        /// The original path
        from: String,
        /// The new path
        to: String,
    },
}

/// Metadata relating to an event that can be used to to identify the user and session. A sequence
/// number is also included to allow ordering in systems where event ordering is not guaranteed.
#[derive(Debug, Clone)]
pub struct EventMeta {
    /// The user this event pertains to. A user may have more than one connection or session.
    pub username: String,
    /// Identifies a single session pertaining to a connected client.
    pub trace_id: String,
    /// The event sequence number as incremented per session.
    pub sequence_number: u64,
}

/// An listener for [`DataEvent`](crate::notification::DataEvent)s. Implementations can
/// be passed to [`ServerBuilder::notify_data`](crate::ServerBuilder::notify_data)
/// in order to receive notifications.
#[async_trait]
pub trait DataListener: Sync + Send + Debug {
    /// Called after the event happened. Event metadata is also passed to allow pinpointing the user
    /// session for which it happened.
    async fn receive_data_event(&self, e: DataEvent, m: EventMeta);
}

/// A listener for [`PresenceEvent`](crate::notification::PresenceEvent)s. Implementations can
/// be passed to [`ServerBuilder::notify_presence`](crate::ServerBuilder::notify_presence)
/// in order to receive notifications.
#[async_trait]
pub trait PresenceListener: Sync + Send + Debug {
    /// Called after the event happened. Event metadata is also passed to allow pinpointing the user
    /// session for which it happened.
    async fn receive_presence_event(&self, e: PresenceEvent, m: EventMeta);
}

#[async_trait]
impl DataListener for Box<dyn DataListener> {
    async fn receive_data_event(&self, e: DataEvent, m: EventMeta) {
        self.as_ref().receive_data_event(e, m).await
    }
}

#[async_trait]
impl PresenceListener for Box<dyn PresenceListener> {
    async fn receive_presence_event(&self, e: PresenceEvent, m: EventMeta) {
        self.as_ref().receive_presence_event(e, m).await
    }
}

#[async_trait]
impl DataListener for Arc<dyn DataListener> {
    async fn receive_data_event(&self, e: DataEvent, m: EventMeta) {
        self.as_ref().receive_data_event(e, m).await
    }
}

#[async_trait]
impl PresenceListener for Arc<dyn PresenceListener> {
    async fn receive_presence_event(&self, e: PresenceEvent, m: EventMeta) {
        self.as_ref().receive_presence_event(e, m).await
    }
}
