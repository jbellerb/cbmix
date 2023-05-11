use axum::extract::ws::{Message as WsMessage, WebSocket};
use cbmix_admin_proto::{message::Message, GraphServiceRequest};
use prost::Message as MessageTrait;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Underlying WebSocket error")]
    Socket,
    #[error("Received unexpected WebSocket message")]
    UnexpectedMessage,
    #[error("Unable to decode protobuf message")]
    Decode,
}

pub(super) async fn next(
    socket: &mut WebSocket,
) -> Option<Result<(u32, GraphServiceRequest), Error>> {
    match socket.recv().await {
        Some(Ok(msg)) => match msg {
            WsMessage::Binary(raw) => {
                let message = Message::decode(&*raw).map_err(|e| {
                    error!("error parsing message: {}", e);
                    Error::Decode
                });

                match message {
                    Ok(m) => Some(Message::get_request(&m).map_err(|e| {
                        error!("error reading message: {}", e);
                        Error::Decode
                    })),
                    Err(e) => Some(Err(e)),
                }
            }
            WsMessage::Close(_) => None,
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

pub(super) async fn send(socket: &mut WebSocket, message: Message) -> Result<(), Error> {
    socket
        .send(WsMessage::Binary(message.encode_to_vec()))
        .await
        .map_err(|e| {
            error!("websocket error: {}", e);
            Error::Socket
        })
}
