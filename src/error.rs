use axum::{http::StatusCode, response::IntoResponse};
use thiserror::Error;
use uuid::Uuid;


pub type Result<T> = std::result::Result<T, LTZFError>;

#[derive(Error, Debug)] 
pub enum DataValidationError{
}

// catch-all error Enumeration for the whole application
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum LTZFError {
        
    #[error("Uuid data was received in the wrong format: {0}")]
    UuidError(#[from] uuid::Error),
    #[error("Header Could not be converted to String{0}")]
    HeaderToStringError(#[from] axum::http::header::ToStrError),

    #[error("Required Field Missing to complete Insert: {0}")]
    MissingFieldForInsert(String), 

    #[error("The same API Id was already supplied: {0}")]
    ApiIDEqual(uuid::Uuid),

    #[error("Multiple Merge Candidates found: {0:?}")]
    MultipleMergeCandidates(Vec<Uuid>),

    #[error("Database Operation Error: {0}")]
    DieselError(#[from] diesel::result::Error),

    #[error("Database Interaction Error: {0}")]
    DeadpoolDieselError(#[from] deadpool_diesel::InteractError),

    #[error("Database Connection error: {0}")]
    DeadpoolPoolError(#[from] deadpool_diesel::PoolError),
    
    #[error("Database Migrations error: {0}")]
    DieselMigrationsError(#[from] diesel_migrations::MigrationError),
    #[error("Deadpool Build Error: {0}")]
    DeadpoolBuildError(#[from] deadpool::managed::BuildError),

    #[error("Server Error: {0}")]
    ServerError(#[from] axum::Error),

    #[error("Hardware Error: {0}")]
    HardwareError(#[from] std::io::Error),

    #[error("Mail Error: {0}")]
    MailError(#[from] lettre::transport::smtp::Error),
}

impl IntoResponse for LTZFError {
    fn into_response(self) -> axum::response::Response {
        match self{
            LTZFError::UuidError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            LTZFError::HeaderToStringError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            LTZFError::MissingFieldForInsert(_) => StatusCode::UNPROCESSABLE_ENTITY,
            LTZFError::ApiIDEqual(_) => StatusCode::BAD_REQUEST,
            LTZFError::MultipleMergeCandidates(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }.into_response()
    }
}