use std::sync::Arc;

use crate::{
    notification,
    notification::DataListener,
    notification::event::PresenceListener,
    server::ControlChanMsg,
    server::session::TraceId,
    server::{
        Event, Reply,
        controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
    },
};

use async_trait::async_trait;

// Control channel middleware that detects data changes and dispatches data change events to a inner
// [DataChangeEventListener](crate::notification::DataChangeEventListener).
pub struct EventDispatcherMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    data_listener: Arc<dyn DataListener>,
    presence_listener: Arc<dyn PresenceListener>,
    next: Next,
    sequence_nr: u64,
    username: String,
    trace_id: TraceId,
}

impl<Next> EventDispatcherMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    pub fn new(data_listener: Arc<dyn DataListener>, presence_listener: Arc<dyn PresenceListener>, next: Next) -> Self {
        EventDispatcherMiddleware {
            data_listener,
            presence_listener,
            next,
            sequence_nr: 0,
            username: "unknown".to_string(),
            trace_id: TraceId::new(),
        }
    }
}

#[async_trait]
impl<Next> ControlChanMiddleware for EventDispatcherMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        let events = if let Event::InternalMsg(msg) = &event {
            let presence_event = match msg {
                ControlChanMsg::AuthSuccess { username, trace_id } => {
                    self.username.clone_from(username);
                    self.trace_id = *trace_id;
                    Some(notification::PresenceEvent::LoggedIn)
                }
                ControlChanMsg::ExitControlLoop => Some(notification::PresenceEvent::LoggedOut),
                _ => None,
            };
            let data_event = match msg {
                ControlChanMsg::SentData { path, bytes } => Some(notification::DataEvent::Got {
                    path: String::from(path),
                    bytes: *bytes,
                }),
                ControlChanMsg::WrittenData { path, bytes } => Some(notification::DataEvent::Put {
                    path: String::from(path),
                    bytes: *bytes,
                }),
                ControlChanMsg::RmDirSuccess { path } => Some(notification::DataEvent::RemovedDir { path: String::from(path) }),
                ControlChanMsg::DelFileSuccess { path } => Some(notification::DataEvent::Deleted { path: String::from(path) }),
                ControlChanMsg::MkDirSuccess { path } => Some(notification::DataEvent::MadeDir { path: String::from(path) }),
                ControlChanMsg::RenameSuccess { old_path, new_path } => Some(notification::DataEvent::Renamed {
                    from: old_path.clone(),
                    to: new_path.clone(),
                }),
                _ => None,
            };
            (data_event, presence_event)
        } else {
            (None, None)
        };

        match events {
            (None, None) => {}
            _ => {
                self.sequence_nr += 1;
                let m = notification::EventMeta {
                    username: self.username.clone(),
                    trace_id: self.trace_id.to_string(),
                    sequence_number: self.sequence_nr,
                };
                match events {
                    (Some(event), None) => self.data_listener.receive_data_event(event, m).await,
                    (None, Some(event)) => self.presence_listener.receive_presence_event(event, m).await,
                    _ => {}
                }
            }
        }

        self.next.handle(event).await
    }
}
