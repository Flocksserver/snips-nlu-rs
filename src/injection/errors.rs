use thiserror::Error;

#[derive(Debug, Error)]
pub enum NluInjectionErrorKind {
    #[error("Entity is not injectable: {msg:?}")]
    EntityNotInjectable { msg: String },
    #[error("Internal injection error: {msg:?}")]
    InternalInjectionError { msg: String },
}
