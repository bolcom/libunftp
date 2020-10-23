//! Contains code pertaining to the FTP *control* channel/connection.

pub mod command;
use command::Command;

pub(crate) mod handler;

pub(super) mod commands;

mod parse_error;

pub(crate) mod event;
pub(crate) use event::Event;

mod codecs;

pub(crate) mod reply;
pub(crate) use reply::{Reply, ReplyCode};

mod error;
pub(crate) use error::ControlChanErrorKind;

mod control_loop;
pub(crate) use control_loop::{spawn as spawn_loop, Config as LoopConfig};

mod auth;
mod ftps;
mod log;
mod middleware;
