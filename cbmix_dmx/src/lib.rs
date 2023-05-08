use std::collections::HashMap;

use cbmix_admin_proto::Scene;
use cbmix_common::shutdown;
use cbmix_graph::Event;
use ola::{connect_async, DmxBuffer, StreamingClientAsync};
use thiserror::Error;
use tokio::{net::TcpStream, sync::mpsc};
use tracing::{debug, error, trace};
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
    mixer_tx: mpsc::Sender<Event>,
    subscription: mpsc::Sender<DmxBuffer>,
    mixer_rx: mpsc::Receiver<DmxBuffer>,
    universes: HashMap<Uuid, u32>,
    shutdown: shutdown::Receiver,
}

impl Dmx {
    pub async fn new(
        mixer_tx: mpsc::Sender<Event>,
        shutdown: shutdown::Receiver,
    ) -> Result<Self, Error> {
        let (subscription, mixer_rx) = mpsc::channel(OUTGOING_BUFFER_SIZE);
        let universes = HashMap::new();

        let client = connect_async().await?;

        Ok(Self {
            client,
            subscription,
            mixer_tx,
            mixer_rx,
            universes,
            shutdown,
        })
    }

    pub async fn add_universe(&mut self, universe: u32, id: Uuid) -> Result<(), Error> {
        self.universes.insert(id, universe);

        self.mixer_tx
            .send(Event::Subscribe(id, self.subscription.clone()))
            .await
            .map_err(|_| Error::Mixer)?;

        Ok(())
    }

    pub async fn serve(mut self) {
        loop {
            let update = tokio::select! {
                update = self.mixer_rx.recv() => match update {
                    Some(update) => update,
                    None => return,
                },
                _ = self.shutdown.recv() => return,
            };

            trace!("update: {:?}", update);
        }
    }

    pub async fn update_scene(&mut self, scene: &Scene) -> Result<(), Error> {
        if let Some(universe) = &scene.universe {
            let buffer = DmxBuffer::try_from(universe.clone());
            if let Ok(buffer) = buffer {
                self.client.send_dmx(1, &buffer).await?;
                trace!("sent buffer to ola: {:?}", buffer);
            } else {
                error!("scene event contained invalid universe buffer");
            }
        } else {
            debug!("recieved scene with no change, ignoring");
        }

        Ok(())
    }
}
