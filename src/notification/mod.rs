#![deny(missing_docs)]
//!
//! Allows users to listen to events emitted by libunftp.
//!
//! To listen for changes in data implement the [`DataListener`]
//! trait and use the [`Server::notify_data`](crate::Server::notify_data) method
//! to make libunftp notify it.
//!
//! To listen to logins and logouts implement the [`PresenceListener`]
//! trait and use the [`Server::notify_presence`](crate::Server::notify_data) method
//! to make libunftp use it.
//!

pub(crate) mod event;
pub(crate) mod nop;

pub use event::{DataEvent, DataListener, EventMeta, PresenceEvent, PresenceListener};
