use std::collections::HashMap;

use crate::transaction::MapLike;

use generational_arena::Index;
use ola::DmxBuffer;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Error, Clone, Debug)]
pub enum Error {
    #[error("Channel unexpectedly closed")]
    ChannelClosed,
    #[error("Input node is missing")]
    NoInput,
}

#[derive(Clone, Debug)]
pub enum GraphUpdate {
    Update { id: Uuid, channels: DmxBuffer },
    Closed { id: Uuid },
}

#[derive(Clone, Debug)]
pub struct Subscription {
    id: Uuid,
    pub input: Uuid,
    pub index: Index,
    data: DmxBuffer,
    channel: mpsc::Sender<GraphUpdate>,
}

impl Subscription {
    pub async fn new(
        id: Uuid,
        input: Uuid,
        index: Index,
        states: &HashMap<Uuid, DmxBuffer>,
        channel: mpsc::Sender<GraphUpdate>,
    ) -> Result<Self, Error> {
        if let Some(data) = states.get(&input) {
            let subscription = Self {
                id,
                input,
                index,
                data: data.clone(),
                channel,
            };

            subscription.send_state().await?;

            Ok(subscription)
        } else {
            Err(Error::NoInput)
        }
    }

    pub async fn update<S>(&mut self, states: &S) -> Result<(), Error>
    where
        S: MapLike<Uuid, DmxBuffer>,
    {
        if let Some(updated) = states.get(&self.input) {
            if updated != &self.data {
                self.data = updated.clone();
                self.send_state().await?;
            }

            Ok(())
        } else {
            error!(
                "node {} missing while updating subscription {}",
                self.input, self.id
            );
            Err(Error::NoInput)
        }
    }

    async fn send_state(&self) -> Result<(), Error> {
        self.channel
            .send(GraphUpdate::Update {
                id: self.id,
                channels: self.data.clone(),
            })
            .await
            .map_err(|_| {
                error!(
                    "channel found closed while updating subscription {}",
                    self.id
                );
                Error::ChannelClosed
            })
    }

    pub async fn close(self) {
        if self
            .channel
            .send(GraphUpdate::Closed { id: self.id })
            .await
            .is_err()
        {
            warn!("subscription {} already closed", self.id)
        }
    }
}
