use super::command::Command;
use crate::server::ControlChanMsg;

/// Event represents a control channel `Event` that will be handled by our per-client control
/// channel event loop. It can either be a command from the client or an internal message like a
/// status message from the data channel handler.
#[derive(Debug)]
pub enum Event {
    /// A command from a client (e.g. `USER` or `PASV`)
    Command(Command),
    /// A message originating from within the library
    InternalMsg(ControlChanMsg),
}
