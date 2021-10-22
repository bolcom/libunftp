//! Contains code pertaining to the FTP *control* channel/connection.

pub mod command;

pub(crate) mod event;
pub(crate) mod handler;
pub(crate) mod reply;

pub(super) mod commands;

mod auth;
mod codecs;
mod control_loop;
mod error;
mod ftps;
mod line_parser;
mod log;
mod middleware;

use command::Command;
pub(crate) use control_loop::{spawn as spawn_loop, Config as LoopConfig};
pub(crate) use error::{ControlChanError, ControlChanErrorKind};
pub(crate) use event::Event;
pub(crate) use middleware::ControlChanMiddleware;
pub use reply::ServerState;
pub(crate) use reply::{Reply, ReplyCode};
