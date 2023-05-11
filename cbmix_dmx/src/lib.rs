use std::collections::HashMap;

use cbmix_common::shutdown;
use cbmix_graph::{GraphHandle, GraphUpdate, Node};
use ola::{client::ClientAsync, connect_async, DmxBuffer};
use thiserror::Error;
use tokio::{net::TcpStream, sync::mpsc};
use tracing::{error, trace, warn};
use uuid::Uuid;

const OUTGOING_BUFFER_SIZE: usize = 15;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to connect to DMX: {0}")]
    DmxConnect(#[from] ola::config::ConnectError),
    #[error("Failed to send DMX message: {0}")]
    DmxCall(#[from] ola::client::CallError),
    #[error("Failed to subscribe to graph")]
    Subscribe,
    #[error("Failed to update graph with new input")]
    Insert,
}

pub struct Dmx {
    client: ClientAsync<TcpStream>,
    graph: GraphHandle,
    subscription: mpsc::Sender<GraphUpdate>,
    graph_rx: mpsc::Receiver<GraphUpdate>,
    inputs: HashMap<u32, Uuid>,
    outputs: HashMap<Uuid, u32>,
    shutdown: shutdown::Receiver,
}

impl Dmx {
    pub async fn new(graph: GraphHandle, shutdown: shutdown::Receiver) -> Result<Self, Error> {
        let (subscription, graph_rx) = mpsc::channel(OUTGOING_BUFFER_SIZE);

        let client = connect_async().await?;

        Ok(Self {
            client,
            subscription,
            graph,
            graph_rx,
            outputs: HashMap::new(),
            inputs: HashMap::new(),
            shutdown,
        })
    }

    pub async fn add_output(&mut self, universe: u32, id: Uuid) -> Result<(), Error> {
        let subscription = self
            .graph
            .subscribe(id, self.subscription.clone())
            .await
            .map_err(|_| Error::Subscribe)?;
        self.outputs.insert(subscription, universe);

        // the main loop isn't running yet, so we need to handle the first
        // event sent after subscribing to avoid building up backpressure
        if let Some(update) = self.graph_rx.recv().await {
            self.handle_update(update).await;
        }

        Ok(())
    }

    pub async fn add_input(&mut self, universe: u32, id: Uuid) -> Result<(), Error> {
        let node = Node::Input {
            channels: DmxBuffer::new(),
        };

        self.graph
            .insert(id, node)
            .await
            .map_err(|_| Error::Insert)?;

        self.client.register_universe(universe).await?;
        self.inputs.insert(universe, id);

        Ok(())
    }

    pub async fn serve(mut self) {
        loop {
            tokio::select! {
                update = self.graph_rx.recv() => match update {
                    Some(update) => self.handle_update(update).await,
                    None => {
                        error!("connection with graph unexpectedly closed");
                        break
                    },
                },
                update = self.client.recv() => match update {
                    Ok((universe, data)) => self.update_input(universe as u32, data).await,
                    Err(e) => {
                        error!("error occured while receiving from ola: {:?}", e);
                    },
                },
                _ = self.shutdown.recv() => break,
            };
        }

        self.shutdown.force_shutdown().await
    }

    async fn handle_update(&mut self, update: GraphUpdate) {
        match update {
            GraphUpdate::Update { id, channels } => {
                if let Some(universe) = self.outputs.get(&id) {
                    if let Err(e) = self.client.send_dmx_streaming(*universe, &channels).await {
                        error!("failed to update universe: {}", e);
                    }
                    trace!("sent buffer to ola: {:?}", channels);
                } else {
                    warn!("recieved update from unknown output {}", id);
                }
            }
            GraphUpdate::Closed { id } => {
                error!("dmx subscription {} closed", id)
            }
        }
    }

    async fn update_input(&mut self, universe: u32, data: DmxBuffer) {
        if let Some(id) = self.inputs.get(&universe) {
            if let Err(e) = self.graph.insert(*id, Node::Input { channels: data }).await {
                error!("failed to send dmx input update to graph: {}", e);
            }
        } else {
            warn!("recieved dmx update for unknown univers {}", universe);
        }
    }
}
