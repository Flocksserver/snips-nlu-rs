use thiserror::Error;

#[derive(Debug, Error)]
pub enum SnipsNluError {
    #[error("Unable to read file '{0}'")]
    ModelLoad(String),
    #[error("Mismatched model version: model is {model:?} but runner is {runner:?}")]
    WrongModelVersion { model: String, runner: &'static str },
    #[error("Unknown intent: '{0}'")]
    UnknownIntent(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}
