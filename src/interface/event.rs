use axum::extract::ws::{Message, WebSocket};
use tracing::{error, info};

#[derive(Debug)]
pub enum Event {
    WsMessage(Message),
}

pub(super) async fn next(socket: &mut WebSocket) -> Option<Event> {
    match socket.recv().await {
        Some(Ok(msg)) => Some(Event::WsMessage(msg)),
        Some(Err(e)) => {
            error!("websocket error: {}", e);
            None
        }
        None => {
            info!("client has disconnected");
            None
        }
    }
}
