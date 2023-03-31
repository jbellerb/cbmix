use crate::config::OutputConfig;
use crate::event::Event;
use crate::shutdown;

use tokio::sync::{broadcast, mpsc};
use tracing::info;

const INCOMING_BUFFER_SIZE: usize = 30;
const OUTGOING_BUFFER_SIZE: usize = 15;

pub struct DmxStage {
    config: OutputConfig,
    shutdown: shutdown::Receiver,
    incoming_tx: mpsc::Sender<Event>,
    incoming_rx: mpsc::Receiver<Event>,
    outgoing_tx: broadcast::Sender<Event>,
}

impl DmxStage {
    pub fn new(config: OutputConfig, shutdown: shutdown::Receiver) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_BUFFER_SIZE);
        let (outgoing_tx, _) = broadcast::channel(OUTGOING_BUFFER_SIZE);

        Self {
            config,
            shutdown,
            incoming_tx,
            incoming_rx,
            outgoing_tx,
        }
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
                    info!("recieved: {:?}", event);
                }
                Event::SceneSwitch(event) => {
                    info!("recieved: {:?}", event);
                }
            }
        }
    }
}
