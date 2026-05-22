use thiserror::Error;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Recipient not found: {0}")]
    RecipientNotFound(String),

    #[error("Invalid phone number format: {0}")]
    InvalidPhoneNumber(String),

    #[error("Group not found: {0}")]
    GroupNotFound(String),

    #[error("Command '{0}' is not yet implemented")]
    NotImplemented(String),

    #[error("Library limitation: {0}")]
    LibraryLimitation(String),

    #[error("Store error: {0}")]
    StoreError(String),

    #[error("Signal server error: {0}")]
    ServerError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Anyhow error: {0}")]
    Other(#[from] anyhow::Error),
}

impl SignalError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            SignalError::NotImplemented(_) => -32601, // Method not found
            SignalError::AccountNotFound(_) => -32001,
            SignalError::RecipientNotFound(_) => -32002,
            SignalError::InvalidPhoneNumber(_) => -32003,
            SignalError::GroupNotFound(_) => -32004,
            SignalError::LibraryLimitation(_) => -32005,
            SignalError::StoreError(_) => -32006,
            SignalError::ServerError(_) => -32007,
            _ => -32603, // Internal error
        }
    }
}
