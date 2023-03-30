use crate::proto::cbmix::message::{Message, MessageType};
use crate::proto::cbmix::SceneUpdateEvent;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use prost::Message as MessageTrait;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying WebSocket error")]
    Socket,
    #[error("Recieved unexpected WebSocket message")]
    UnexpectedMessage,
    #[error("Unable to decode protobuf message")]
    Decode,
    #[error("Recieved unexpected message type")]
    UnexpectedType,
    #[error("Recieved unknown event type")]
    UnknownEvent,
    #[error("Recieved incomplete event")]
    IncompleteEvent,
}

#[derive(Debug)]
pub enum Event {
    SceneUpdate(SceneUpdateEvent),
}

impl TryFrom<Message> for Event {
    type Error = Error;

    fn try_from(value: Message) -> Result<Self, Self::Error> {
        if value.r#type() != MessageType::Event {
            error!("recieved message of type {}", value.r#type);
            return Err(Error::UnexpectedType);
        }

        if let (Some(name), Some(body)) = (value.name, value.body) {
            match name.as_str() {
                "SceneUpdate" => Ok(Event::SceneUpdate(
                    SceneUpdateEvent::decode(&*body).map_err(|e| {
                        error!("error parsing scene update event: {}", e);
                        Error::Decode
                    })?,
                )),
                _ => {
                    error!("recieved unknown \"{}\" event", name);
                    Err(Error::UnknownEvent)
                }
            }
        } else {
            error!("recieved incomplete event");
            Err(Error::IncompleteEvent)
        }
    }
}

pub(super) async fn next(socket: &mut WebSocket) -> Option<Result<Event, Error>> {
    match socket.recv().await {
        Some(Ok(msg)) => match msg {
            WsMessage::Binary(raw) => {
                let message = Message::decode(&*raw).map_err(|e| {
                    error!("error parsing message: {}", e);
                    Error::Decode
                });

                match message {
                    Ok(m) => Some(m.try_into()),
                    Err(e) => Some(Err(e)),
                }
            }
            _ => {
                warn!("recieved unexpected websocket message: {:?}", msg);
                Some(Err(Error::UnexpectedMessage))
            }
        },
        Some(Err(e)) => {
            error!("websocket error: {}", e);
            Some(Err(Error::Socket))
        }
        None => {
            info!("client has disconnected");
            None
        }
    }
}
