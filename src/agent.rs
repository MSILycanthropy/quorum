//! An ACP agent, and the record of what it has said and done.

mod session;
mod transcript;

pub use session::{Session, Spawn};
pub use transcript::{
    Call, Choice, Command, Consequence, Cost, Ending, Entry, Permission, Progress, Setting, State,
    Task, Transcript, Update, Usage,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot launch the agent {command}: {message}")]
    Spawn { command: String, message: String },
    #[error(transparent)]
    Protocol(#[from] agent_client_protocol::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
