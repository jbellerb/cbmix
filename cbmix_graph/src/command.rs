use crate::Error;

use generational_arena::Index;
use ola::DmxBuffer;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug)]
pub enum Command {
    CreateInput {
        id: Uuid,
        channels: DmxBuffer,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    CreateOutput {
        id: Uuid,
        input: Uuid,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    Remove {
        id: Uuid,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    Subscribe {
        id: Uuid,
        subscriber: mpsc::Sender<(Uuid, DmxBuffer)>,
        callback: oneshot::Sender<Result<Index, Error>>,
    },
    Unsubscribe {
        id: Uuid,
        index: Index,
        callback: oneshot::Sender<Result<(), Error>>,
    },
}
