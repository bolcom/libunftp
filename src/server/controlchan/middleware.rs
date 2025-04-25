use crate::server::{
    Event,
    controlchan::{Reply, error::ControlChanError},
};
use async_trait::async_trait;
use std::{future::Future, pin::Pin};

// Defines the requirements for code that wants to intercept and do something with control channel events.
#[async_trait]
pub trait ControlChanMiddleware: Send + Sync {
    // Handles the specified `Event` and returns a `Reply` for the user or a `ControlChanError` if
    // some unexpected error occurred.
    async fn handle(&mut self, e: Event) -> Result<Reply, ControlChanError>;
}

// Allows plain functions to be middleware. Experimental...
#[async_trait]
impl<Function> ControlChanMiddleware for Function
where
    Function: Send + Sync + Fn(Event) -> Pin<Box<dyn Future<Output = Result<Reply, ControlChanError>> + Send>>,
{
    async fn handle(&mut self, e: Event) -> Result<Reply, ControlChanError> {
        (self)(e).await
    }
}
