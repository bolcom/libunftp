//! Contains code pertaining to the FTP *control* channel

pub mod command;
use command::Command;

pub(crate) mod handler;

pub(super) mod commands;

mod parse_error;

pub(crate) mod event;
pub(crate) use event::Event;

pub(crate) mod codecs;
pub(crate) use codecs::FTPCodec;

pub(crate) mod reply;
pub(crate) use reply::{Reply, ReplyCode};

mod error;
pub(super) use error::ControlChanError;
pub(crate) use error::ControlChanErrorKind;
