//! Contains code pertaining to the FTP *control* channel/connection.

pub mod command;

pub(crate) mod event;
pub(crate) mod handler;
pub(crate) mod reply;

pub(super) mod commands;

mod active_passive;
mod auth;
mod codecs;
mod control_loop;
mod error;
mod ftps;
mod line_parser;
mod log;
mod middleware;
mod notify;

use command::Command;
pub(crate) use control_loop::{Config as LoopConfig, spawn as spawn_loop};
pub(crate) use error::{ControlChanError, ControlChanErrorKind};
pub(crate) use event::Event;
pub(crate) use middleware::ControlChanMiddleware;
pub(crate) use reply::{Reply, ReplyCode};
