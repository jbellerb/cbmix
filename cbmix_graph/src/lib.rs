mod command;
mod graph;
mod handle;

use command::Command;
use graph::Node;
use graph::SceneGraph;
pub use handle::GraphHandle;

use cbmix_common::shutdown;
pub use generational_arena::{Arena, Index};
use ola::DmxBuffer;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};
use uuid::Uuid;

// UUID namespace ID for scene graph nodes
pub const NAMESPACE_SCENE: Uuid = Uuid::from_u128(0x57e02364b4e34787ae21bf4dde8dbdef);

const INCOMING_BUFFER_SIZE: usize = 30;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unable to insert node in graph")]
    Insert(#[from] graph::Error),
    #[error("No output with id {0}")]
    Subscribe(Uuid),
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
                Command::CreateInput {
                    id,
                    channels,
                    callback,
                } => {
                    _ = callback.send(self.create_input(id, channels));
                }
                Command::CreateOutput {
                    id,
                    input,
                    callback,
                } => {
                    _ = callback.send(self.create_output(id, input));
                }
                Command::Remove { id, callback } => {
                    _ = callback.send(self.remove(id));
                }
                Command::Subscribe {
                    id,
                    subscriber,
                    callback,
                } => {
                    _ = callback.send(self.subscribe(id, subscriber));
                }
                Command::Unsubscribe {
                    id,
                    index,
                    callback,
                } => {
                    _ = callback.send(self.unsubscribe(id, index));
                }
            }
        }
    }

    fn create_input(&mut self, id: Uuid, channels: DmxBuffer) -> Result<(), Error> {
        debug!("creating input {:?}", id);
        self.graph
            .insert(
                id,
                Node::Input {
                    outputs: Vec::new(),
                    channels,
                },
            )
            .map_err(Error::Insert)
    }

    fn create_output(&mut self, id: Uuid, input: Uuid) -> Result<(), Error> {
        debug!("creating output {:?}", id);
        self.graph
            .insert(
                id,
                Node::Output {
                    input: Some(input),
                    subscribers: Arena::new(),
                },
            )
            .map_err(Error::Insert)
    }

    fn remove(&mut self, id: Uuid) -> Result<(), Error> {
        debug!("removing node {:?}", id);
        Ok(self.graph.remove(id)?)
    }

    fn subscribe(
        &mut self,
        id: Uuid,
        subscriber: mpsc::Sender<(Uuid, DmxBuffer)>,
    ) -> Result<Index, Error> {
        if let Some(Node::Output { subscribers, .. }) = self.graph.node_weight_mut(id) {
            debug!("registering new subscriber with output {}", id);
            Ok(subscribers.insert(subscriber))
        } else {
            Err(Error::Subscribe(id))
        }
    }

    fn unsubscribe(&mut self, id: Uuid, index: Index) -> Result<(), Error> {
        if let Some(Node::Output { subscribers, .. }) = self.graph.node_weight_mut(id) {
            debug!("removing subscriber from output {}", id);
            if subscribers.remove(index).is_none() {
                warn!("subscriber was not found on output");
            }

            Ok(())
        } else {
            Err(Error::Subscribe(id))
        }
    }
}
