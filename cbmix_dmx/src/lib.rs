use std::collections::HashMap;

use cbmix_common::shutdown;
use cbmix_graph::GraphHandle;
use ola::{connect_async, DmxBuffer, StreamingClientAsync};
use thiserror::Error;
use tokio::{net::TcpStream, sync::mpsc};
use tracing::{error, trace, warn};
use uuid::Uuid;

const OUTGOING_BUFFER_SIZE: usize = 15;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying DMX error: {0}")]
    Dmx(#[from] ola::Error),
    #[error("Failed to subscribe to mixer")]
    Mixer,
}

pub struct Dmx {
    client: StreamingClientAsync<TcpStream>,
    graph: GraphHandle,
    subscription: mpsc::Sender<(Uuid, DmxBuffer)>,
    graph_rx: mpsc::Receiver<(Uuid, DmxBuffer)>,
    universes: HashMap<Uuid, u32>,
    shutdown: shutdown::Receiver,
}

impl Dmx {
    pub async fn new(graph: GraphHandle, shutdown: shutdown::Receiver) -> Result<Self, Error> {
        let (subscription, graph_rx) = mpsc::channel(OUTGOING_BUFFER_SIZE);
        let universes = HashMap::new();

        let client = connect_async().await?;

        Ok(Self {
            client,
            subscription,
            graph,
            graph_rx,
            universes,
            shutdown,
        })
    }

    pub async fn add_universe(&mut self, universe: u32, id: Uuid) -> Result<(), Error> {
        self.universes.insert(id, universe);

        self.graph
            .subscribe(id, self.subscription.clone())
            .await
            .map_err(|_| Error::Mixer)?;

        Ok(())
    }

    pub async fn serve(mut self) {
        loop {
            let (id, channels) = tokio::select! {
                update = self.graph_rx.recv() => match update {
                    Some(update) => update,
                    None => return,
                },
                _ = self.shutdown.recv() => return,
            };

            match self.update_universe(&id, &channels).await {
                Ok(()) => {}
                Err(e) => error!("failed to update universe: {}", e),
            }
        }
    }

    async fn update_universe(&mut self, id: &Uuid, channels: &DmxBuffer) -> Result<(), Error> {
        if let Some(universe) = self.universes.get(id) {
            self.client.send_dmx(*universe, channels).await?;
            trace!("sent buffer to ola: {:?}", channels);
        } else {
            warn!("recieved update from unknown output {}", id);
        }

        Ok(())
    }
}
