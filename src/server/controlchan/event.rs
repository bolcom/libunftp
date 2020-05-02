use super::command::Command;
use crate::server::InternalMsg;

/// Event represents an `Event` that will be handled by our per-client event loop. It can be either
/// a command from the client, or a status message from the data channel handler.
#[derive(Debug)]
pub enum Event {
    /// A command from a client (e.g. `USER` or `PASV`)
    Command(Command),
    /// A status message from the data channel loop
    InternalMsg(InternalMsg),
}
