use crate::server::{
    controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
    Event, Reply,
};

use async_trait::async_trait;

// Control channel middleware that logs all control channel events
pub struct LoggingMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    pub logger: slog::Logger,
    pub sequence_nr: u64,
    pub next: Next,
}

#[async_trait]
impl<Next> ControlChanMiddleware for LoggingMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        self.sequence_nr += 1;
        slog::info!(self.logger, "Control channel event {:?}", event; "seq" => self.sequence_nr);
        let result = self.next.handle(event).await;
        match &result {
            Ok(reply) => slog::info!(self.logger, "Control channel reply {:?}", reply; "seq" => self.sequence_nr),
            Err(error) => slog::warn!(self.logger, "Control channel error {:?}", error; "seq" => self.sequence_nr),
        };
        result
    }
}
