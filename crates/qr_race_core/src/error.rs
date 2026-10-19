//! Domain errors. Callers match on these; wording is for logs, not users.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid room address: {0}")]
    Address(String),

    #[error("{0} must not be empty")]
    Empty(&'static str),

    #[error("relay payload is {size} bytes, over the {max}-byte limit")]
    RelayTooLarge { size: usize, max: usize },

    #[error("only the host may {action}")]
    NotHost { action: &'static str },

    #[error("the user is not a room member")]
    NotAMember,

    #[error("the user is not a co-host")]
    NotACoHost,

    #[error("the target is already a co-host")]
    AlreadyCoHost,

    #[error("the host is already the host")]
    AlreadyHost,

    #[error("the host cannot leave their own room")]
    HostCannotLeave,

    #[error("the room is already lit")]
    AlreadyLit,

    #[error("invalid module manifest: {0}")]
    Manifest(&'static str),
}
