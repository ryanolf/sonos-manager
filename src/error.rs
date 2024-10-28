use thiserror::*;

#[derive(Error, Debug)]
/// Errors relevant to sonos manager
pub enum Error {
    /// Errors from the core crate
    #[error(transparent)]
    Sonor(#[from] sonor::Error),
    /// Error with subscriptions
    #[error("Error in event subscription: {0}")]
    SubscriberError(String),
    /// If the controller panics or drops the receiver
    #[error("Controller has shut down.")]
    ControllerOffline,
    /// If the controller panics formulation a response
    #[error("Controller has dropped the response sender")]
    MessageRecvError,
    /// Controller not initialized
    #[error("Controller not initialized")]
    ControllerNotInitialized,
    /// Zone does not exist
    #[error("The requested zone name is not valid")]
    ZoneDoesNotExist,
    /// Error encountered on zone action
    #[error("Error encountered performing zone action")]
    ZoneActionError,
    /// Could not parse content
    #[error("Could not find the requested content")]
    ContentNotFound,
}
