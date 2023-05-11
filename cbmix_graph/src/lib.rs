mod command;
mod graph;
mod handle;
mod node;
mod subscription;
mod transaction;

use command::Command;
use graph::SceneGraph;
pub use handle::GraphHandle;
pub use node::Node;
pub use subscription::GraphUpdate;

use cbmix_common::shutdown;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace};
use uuid::Uuid;

// UUID namespace ID for scene graph nodes
pub const NAMESPACE_SCENE: Uuid = Uuid::from_u128(0x57e02364b4e34787ae21bf4dde8dbdef);

const INCOMING_BUFFER_SIZE: usize = 30;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Node does not exist")]
    MissingNode,
    #[error("Subscription does not exist")]
    MissingSubscription,
    #[error("Unable to insert: {0}")]
    Insert(graph::Error),
    #[error("Unable to subscribe: {0}")]
    Subscribe(graph::Error),
    #[error("Unable to send command to graph manager")]
    Send(#[from] mpsc::error::SendError<Command>),
    #[error("Graph manager suddenly stopped responding")]
    Receive(#[from] oneshot::error::RecvError),
}

pub struct Graph {
    graph: SceneGraph,
    incoming_tx: mpsc::Sender<Command>,
    incoming_rx: mpsc::Receiver<Command>,
    shutdown: shutdown::Receiver,
}

impl Graph {
    pub fn new(shutdown: shutdown::Receiver) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_BUFFER_SIZE);

        Self {
            graph: SceneGraph::new(),
            incoming_tx,
            incoming_rx,
            shutdown,
        }
    }

    pub fn handle(&self) -> GraphHandle {
        GraphHandle::new(self.incoming_tx.clone())
    }

    pub async fn serve(mut self) {
        loop {
            let event = tokio::select! {
                event = self.incoming_rx.recv() => match event {
                    Some(event) => event,
                    None => return,
                },
                _ = self.shutdown.recv() => return,
            };

            match event {
                Command::Insert { id, node, callback } => {
                    trace!("inserting node {}: {:?}", id, node);
                    _ = callback.send(self.graph.insert(id, node).await.map_err(Error::Insert));
                }
                Command::Remove { id, callback } => {
                    trace!("removing node {}", id);
                    _ = callback.send(self.graph.remove(id).await.map_err(|_| Error::MissingNode));
                }
                Command::Subscribe {
                    id,
                    subscriber,
                    callback,
                } => {
                    trace!("subscribing to {}", id);
                    _ = callback.send(
                        self.graph
                            .subscribe(id, subscriber)
                            .await
                            .map_err(Error::Subscribe),
                    );
                }
                Command::Unsubscribe { id, callback } => {
                    trace!("unsubscribing from {}", id);
                    _ = callback.send(
                        self.graph
                            .unsubscribe(id)
                            .map_err(|_| Error::MissingSubscription),
                    );
                }
            }
        }
    }
}
