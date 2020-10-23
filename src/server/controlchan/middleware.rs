use crate::server::{
    controlchan::{error::ControlChanError, Reply},
    Event,
};
use async_trait::async_trait;
use std::{future::Future, pin::Pin};

// Defines the requirements for code that wants to intercept and do something with control channel events.
#[async_trait]
pub trait ControlChanMiddleware: Send + Sync {
    async fn handle(&mut self, e: Event) -> Result<Reply, ControlChanError>;

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
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
