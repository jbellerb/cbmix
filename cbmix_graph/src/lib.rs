mod event;
mod graph;

pub use event::Event;
pub use graph::Node;
use graph::SceneGraph;

use cbmix_admin_proto::SceneUpdateEvent;
use cbmix_common::shutdown;
use ola::DmxBuffer;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

// UUID namespace ID for scene graph nodes
pub const NAMESPACE_SCENE: Uuid = Uuid::from_u128(0x57e02364b4e34787ae21bf4dde8dbdef);

const INCOMING_BUFFER_SIZE: usize = 30;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying DMX error: {0}")]
    Dmx(#[from] ola::Error),
    #[error("Unable to insert node in graph")]
    Insert(#[from] graph::Error),
}

pub struct Graph {
    graph: SceneGraph,
    incoming_tx: mpsc::Sender<Event>,
    incoming_rx: mpsc::Receiver<Event>,
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

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.incoming_tx.clone()
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
                Event::CreateInput(id, channels, callback) => {
                    _ = callback.send(self.create_input(id, channels));
                }
                Event::CreateOutput(id, from, callback) => {
                    _ = callback.send(self.create_output(id, from));
                }
                Event::Subscribe(output, channel) => {
                    debug!("subscribe to {:?}", output);
                    let _ = self.graph.subscribe(&output, channel);
                }
                Event::SceneUpdate(event) => {
                    if let SceneUpdateEvent { scene: Some(scene) } = event {
                        debug!("recieved: {:?}", scene);
                    } else {
                        debug!("received empty scene update event, ignoring");
                    }
                }
                Event::SceneSwitch(event) => {
                    info!("recieved: {:?}", event);
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

    fn create_output(&mut self, id: Uuid, from: Uuid) -> Result<(), Error> {
        debug!("creating output {:?}", id);
        self.graph
            .insert(
                id,
                Node::Output {
                    input: from,
                    subscribers: Vec::new(),
                },
            )
            .map_err(Error::Insert)
    }
}
