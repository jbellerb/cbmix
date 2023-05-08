use crate::command::Command;
use crate::Error;

use generational_arena::Index;
use ola::DmxBuffer;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct GraphHandle {
    graph_tx: mpsc::Sender<Command>,
}

impl GraphHandle {
    pub fn new(graph_tx: mpsc::Sender<Command>) -> Self {
        Self { graph_tx }
    }

    pub async fn create_input(&self, id: Uuid, channels: DmxBuffer) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::CreateInput {
                id,
                channels,
                callback: tx,
            })
            .await?;

        rx.await?
    }

    pub async fn create_output(&self, id: Uuid, input: Uuid) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::CreateOutput {
                id,
                input,
                callback: tx,
            })
            .await?;

        rx.await?
    }

    pub async fn remove(&self, id: Uuid) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::Remove { id, callback: tx })
            .await?;

        rx.await?
    }

    pub async fn subscribe(
        &self,
        id: Uuid,
        subscriber: mpsc::Sender<(Uuid, DmxBuffer)>,
    ) -> Result<Index, Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::Subscribe {
                id,
                subscriber,
                callback: tx,
            })
            .await?;

        rx.await?
    }

    pub async fn unsubscribe(&self, id: Uuid, index: Index) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::Unsubscribe {
                id,
                index,
                callback: tx,
            })
            .await?;

        rx.await?
    }
}
