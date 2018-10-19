#![deny(missing_docs)]
//! Firetrap, a FTP server library for Rust.

#[macro_use]
extern crate log;

extern crate failure;
#[macro_use] extern crate failure_derive;

#[cfg(test)]
#[macro_use] extern crate pretty_assertions;

/// The actual server protocol and networking.
///
/// [`Server`]: ./server/struct.Server.html
pub mod server;

/// The FTP [`Command`]s types and parding
///
/// [`Command`]: ./commands/struct.Command.html
pub mod commands;

/// The [`Authenticator`] trait (used by the [`Server`] to authenticate users), as
/// well as its implementations (e.g. the [`AnonymousAuthenticator`]).
///
/// [`Authenticator`]: ./auth/trait.Authenticator.html
/// [`AnonymousAuthenticator`]: ./auth/struct.AnonymousAuthenticator.html
pub mod auth;

/// The [`StorageBackend`] trait and its implementations (.e.g. [`Filesystem`]).
///
/// [`StorageBackend`]: ./auth/trait.StorageBackend.html
/// [`Filesystem`]: ./storage/struct.Filesystem.html
pub mod storage;

pub use server::Server;
