use axum::extract::ws::{Message as WsMessage, WebSocket};
use cbmix_admin_proto::{
    message::{Message, MessageType},
    GraphServiceRequest, GraphServiceResponse,
};
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

pub(super) async fn send(
    socket: &mut WebSocket,
    seq: u32,
    response: Result<GraphServiceResponse, crate::Error>,
) -> Result<(), Error> {
    let message = match response {
        Ok(res) => res.to_message(seq),
        Err(e) => Message {
            r#type: MessageType::ResponseError as i32,
            seq: Some(seq),
            name: None,
            body: Some(e.to_string().into_bytes()),
        },
    };

    socket
        .send(WsMessage::Binary(message.encode_to_vec()))
        .await
        .map_err(|e| {
            error!("websocket error: {}", e);
            Error::Socket
        })
}
