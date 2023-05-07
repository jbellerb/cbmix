use std::collections::{BTreeMap, HashMap};

use crate::config::OutputConfig;
use crate::mix::event::Event;
use crate::proto::cbmix::{Scene, SceneUpdateEvent};
use crate::shutdown;

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
    mixer_rx: mpsc::Receiver<DmxBuffer>,
    universes: HashMap<Uuid, u32>,
    shutdown: shutdown::Receiver,
}

impl Dmx {
    pub async fn new(
        config: BTreeMap<Uuid, OutputConfig>,
        mixer_tx: mpsc::Sender<Event>,
        shutdown: shutdown::Receiver,
    ) -> Result<Self, Error> {
        let (subscription, mixer_rx) = mpsc::channel(OUTGOING_BUFFER_SIZE);
        let mut universes = HashMap::new();

        for (id, output) in config.iter() {
            if let OutputConfig::Sacn { universe, .. } = output {
                universes.insert(*id, *universe);
                mixer_tx
                    .send(Event::Subscribe(*id, subscription.clone()))
                    .await
                    .map_err(|_| Error::Mixer)?;
            }
        }

        let client = connect_async().await?;

        Ok(Self {
            client,
            mixer_tx,
            mixer_rx,
            universes,
            shutdown,
        })
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
