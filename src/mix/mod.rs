pub mod event;

use crate::proto::cbmix::SceneUpdateEvent;
use crate::scene::SceneGraph;
use crate::shutdown;
use event::Event;

use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const INCOMING_BUFFER_SIZE: usize = 30;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying DMX error: {0}")]
    Dmx(#[from] ola::Error),
}

pub struct Mixer {
    graph: SceneGraph,
    incoming_tx: mpsc::Sender<Event>,
    incoming_rx: mpsc::Receiver<Event>,
    shutdown: shutdown::Receiver,
}

impl Mixer {
    pub fn new(graph: SceneGraph, shutdown: shutdown::Receiver) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_BUFFER_SIZE);

        Self {
            graph,
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
                Event::Subscribe(output, channel) => {
                    debug!("subscribe to {:?}", output);
                    self.graph.subscribe(&output, channel);
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
}
