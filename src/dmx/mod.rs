use crate::config::OutputConfig;
use crate::event::Event;
use crate::proto::cbmix::{Scene, SceneUpdateEvent};
use crate::shutdown;

use ola::{connect_async, DmxBuffer, StreamingClientAsync};
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tracing::{debug, error, info, trace};

const INCOMING_BUFFER_SIZE: usize = 30;
const OUTGOING_BUFFER_SIZE: usize = 15;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying DMX error: {0}")]
    Dmx(#[from] ola::Error),
}

pub struct DmxStage {
    config: OutputConfig,
    client: StreamingClientAsync<TcpStream>,
    incoming_tx: mpsc::Sender<Event>,
    incoming_rx: mpsc::Receiver<Event>,
    outgoing_tx: broadcast::Sender<Event>,
    shutdown: shutdown::Receiver,
}

impl DmxStage {
    pub async fn new(config: OutputConfig, shutdown: shutdown::Receiver) -> Result<Self, Error> {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_BUFFER_SIZE);
        let (outgoing_tx, _) = broadcast::channel(OUTGOING_BUFFER_SIZE);
        let client = connect_async().await?;

        Ok(Self {
            config,
            client,
            incoming_tx,
            incoming_rx,
            outgoing_tx,
            shutdown,
        })
    }

    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.incoming_tx.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.outgoing_tx.subscribe()
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
                Event::SceneUpdate(event) => {
                    if let SceneUpdateEvent { scene: Some(scene) } = event {
                        self.update_scene(&scene).await;
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

    pub async fn update_scene(&mut self, scene: &Scene) {
        if let Some(universe) = &scene.universe {
            let buffer = DmxBuffer::try_from(universe.clone());
            if let Ok(buffer) = buffer {
                self.client
                    .send_dmx(self.config.universe, &buffer)
                    .await
                    .unwrap();
                trace!("sent buffer to ola: {:?}", buffer);
            } else {
                error!("scene event contained invalid universe buffer");
            }
        } else {
            debug!("recieved scene with no change, ignoring");
        }
    }
}
