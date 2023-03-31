mod message;

use crate::config::InterfaceConfig;
use crate::event::Event;
use crate::shutdown;
use message::{next, Error as MessageError};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc,
};
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, warn, Level};

#[derive(Debug)]
pub struct Interface {
    config: InterfaceConfig,
    event_tx: mpsc::Sender<Event>,
    update_rx: broadcast::Receiver<Event>,
    shutdown: shutdown::Receiver,
}

#[derive(Debug)]
struct ServerState {
    event_tx: mpsc::Sender<Event>,
    update_rx: broadcast::Receiver<Event>,
    shutdown: shutdown::Receiver,
}

// this is technically an invalid implementation of Clone since
// tokio::sync::broadcast is not Clone, but ServerState must be Clone for axum
// to pass it around and Receiver::resubscribe() is close enough to a clone for
// what we need
impl Clone for ServerState {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            update_rx: self.update_rx.resubscribe(),
            shutdown: self.shutdown.clone(),
        }
    }
}

impl Interface {
    pub fn new(
        config: InterfaceConfig,
        event_tx: mpsc::Sender<Event>,
        update_rx: broadcast::Receiver<Event>,
        shutdown: shutdown::Receiver,
    ) -> Self {
        Self {
            config,
            event_tx,
            update_rx,
            shutdown,
        }
    }

    pub async fn serve(mut self) {
        let routes = Router::new().route("/api/ws", get(ws_handler));
        let state = ServerState {
            event_tx: self.event_tx,
            update_rx: self.update_rx,
            shutdown: self.shutdown.clone(),
        };

        let app = routes.with_state(state).layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http()
                    .on_request(DefaultOnRequest::new().level(Level::INFO))
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(LatencyUnit::Micros),
                    ),
            ),
        );

        let server = Server::bind(&self.config.listen_addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(self.shutdown.recv());

        info!("listening on {}", self.config.listen_addr);
        match server.await {
            Ok(()) => {}
            Err(e) => {
                error!("server unexpectedly quit: {}", e);
            }
        };
    }
}

#[axum::debug_handler(state = ServerState)]
async fn ws_handler(State(mut state): State<ServerState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|mut socket| async move {
        loop {
            tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok(event)) => forward_event(event, &mut state.event_tx).await,
                    None | Some(Err(MessageError::Socket)) => return,
                    _ => continue,
                },
                update = state.update_rx.recv() => match update {
                    Ok(event) => unimplemented!("received update event: {:?}", event),
                    Err(RecvError::Lagged(n)) => {
                        warn!("interface missed {} dmx updates", n);
                        continue
                    }
                    Err(RecvError::Closed) => {
                        error!("dmx task closed unexpectedly");
                        break
                    }
                },
                _ = state.shutdown.recv() => break
            }
        }

        if let Err(e) = socket.close().await {
            error!("error closing socket: {}", e);
        }
    })
}

async fn forward_event(event: Event, event_tx: &mut mpsc::Sender<Event>) {
    if event_tx.send(event).await.is_err() {
        error!("dmx task while processing a request");
    };
}
