use std::iter::zip;

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
    Add { a: Option<Uuid>, b: Option<Uuid> },
    Multiply { a: Option<Uuid>, b: Option<Uuid> },
}

impl Node {
    pub fn dependencies(&self) -> Vec<Option<Uuid>> {
        match self {
            Node::Input { .. } => Vec::new(),
            Node::Add { a, b } => vec![*a, *b],
            Node::Multiply { a, b } => vec![*a, *b],
        }
    }

    pub fn update(&self, states: &Transaction<Uuid, DmxBuffer>) -> Result<DmxBuffer, Error> {
        match self {
            Node::Input { channels } => Ok(channels.clone()),
            Node::Add { a, b } => match (
                a.map(|n| states.get(&n).ok_or(Error::NoInput(0))),
                b.map(|n| states.get(&n).ok_or(Error::NoInput(1))),
            ) {
                (Some(a), Some(b)) => Ok(zip(a?.iter(), b?.iter())
                    .map(|(a, b)| a + b)
                    .collect::<Vec<u8>>()
                    .try_into()
                    .unwrap()),
                (Some(a), None) => Ok(a?.clone()),
                (None, Some(b)) => Ok(b?.clone()),
                (None, None) => Ok(DmxBuffer::new()),
            },
            Node::Multiply { a, b } => match (
                a.map(|n| states.get(&n).ok_or(Error::NoInput(0))),
                b.map(|n| states.get(&n).ok_or(Error::NoInput(1))),
            ) {
                (Some(a), Some(b)) => Ok(zip(a?.iter(), b?.iter())
                    .map(|(a, b)| (((*a as u16) * (*b as u16)) / 255) as u8)
                    .collect::<Vec<u8>>()
                    .try_into()
                    .unwrap()),
                _ => Ok(DmxBuffer::new()),
            },
        }
    }

    pub fn unlink(&mut self, index: u32) {
        match self {
            Node::Input { .. } => {}
            Node::Add { a, b } => match index {
                0 => *a = None,
                1 => *b = None,
                _ => {}
            },
            Node::Multiply { a, b } => match index {
                0 => *a = None,
                1 => *b = None,
                _ => {}
            },
        }
    }
}
