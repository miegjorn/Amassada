use thiserror::Error;

#[derive(Debug, Error)]
pub enum AmassadaError {
    #[error("canvas not found: {0}")]
    CanvasNotFound(String),
    #[error("canvas parse error: {0}")]
    CanvasParse(String),
    #[error("budget exhausted: {pool}")]
    BudgetExhausted { pool: String },
    #[error("dispatch error: {0}")]
    Dispatch(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("session error: {0}")]
    Session(String),
    #[error("mission error: {0}")]
    Mission(String),
    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, AmassadaError>;
