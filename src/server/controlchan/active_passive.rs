use crate::{
    options::ActivePassiveMode,
    server::{
        Command, Event, Reply, ReplyCode,
        controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
    },
};
use async_trait::async_trait;

// Control channel middleware that enforces disables Active or Passive mode depending on the
// setting of [`DataConnectionMode`](DataConnectionMode).
pub struct ActivePassiveEnforcerMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    pub mode: ActivePassiveMode,
    pub next: Next,
}

#[async_trait]
impl<Next> ControlChanMiddleware for ActivePassiveEnforcerMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match (self.mode, &event) {
            (ActivePassiveMode::PassiveOnly, Event::Command(Command::Port { .. })) => {
                Ok(Reply::new(ReplyCode::CommandNotImplemented, "Active mode not enabled."))
            }
            (ActivePassiveMode::ActiveOnly, Event::Command(Command::Pasv)) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "Passive mode not enabled.")),
            _ => self.next.handle(event).await,
        }
    }
}
