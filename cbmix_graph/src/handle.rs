use crate::command::Command;
use crate::{Error, GraphUpdate, Node};

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

    pub async fn insert(&self, id: Uuid, node: Node) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::Insert {
                id,
                node,
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
        subscriber: mpsc::Sender<GraphUpdate>,
    ) -> Result<Uuid, Error> {
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

    pub async fn unsubscribe(&self, id: Uuid) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.graph_tx
            .send(Command::Unsubscribe { id, callback: tx })
            .await?;

        rx.await?
    }
}
