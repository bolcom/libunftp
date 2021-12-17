use crate::notification::event::{DataEvent, DataListener, EventMeta, PresenceEvent, PresenceListener};

use async_trait::async_trait;

// An event listener that does nothing. Used as a default Null Object in [`Server`](crate::Server).
#[derive(Debug)]
pub struct NopListener {}

#[async_trait]
impl DataListener for NopListener {
    async fn receive_data_event(&self, _: DataEvent, _: EventMeta) {}
}

#[async_trait]
impl PresenceListener for NopListener {
    async fn receive_presence_event(&self, _: PresenceEvent, _: EventMeta) {}
}
