use crate::transaction::Transaction;

use ola::DmxBuffer;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Clone, Debug)]
pub enum Error {
    #[error("Input node is missing")]
    NoInput(u32),
}

#[derive(Clone, Debug)]
pub enum Node {
    Input { channels: DmxBuffer },
}

impl Node {
    pub fn dependencies(&self) -> Vec<Option<Uuid>> {
        match self {
            Node::Input { .. } => Vec::new(),
        }
    }

    pub fn update(&self, _states: &Transaction<Uuid, DmxBuffer>) -> Result<DmxBuffer, Error> {
        match self {
            Node::Input { channels } => Ok(channels.clone()),
        }
    }

    pub fn unlink(&mut self, _index: u32) {
        match self {
            Node::Input { .. } => {}
        }
    }
}
