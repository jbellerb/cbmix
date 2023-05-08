pub mod config;
mod message;

use config::AdminConfig;
use message::{next, Error as MessageError};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use cbmix_common::shutdown;
use cbmix_graph::Event;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, Level};

#[derive(Clone, Debug)]
pub struct Admin {
    config: AdminConfig,
    mixer_tx: mpsc::Sender<Event>,
    shutdown: shutdown::Receiver,
}

#[derive(Clone, Debug)]
struct ServerState {
    mixer_tx: mpsc::Sender<Event>,
    shutdown: shutdown::Receiver,
}

impl Admin {
    pub fn new(
        config: AdminConfig,
        mixer_tx: mpsc::Sender<Event>,
        shutdown: shutdown::Receiver,
    ) -> Self {
        Self {
            config,
            mixer_tx,
            shutdown,
        }
    }

    pub async fn serve(mut self) {
        let routes = Router::new().route("/api/ws", get(ws_handler));
        let state = ServerState {
            mixer_tx: self.mixer_tx,
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
        let (tx, mut subscription) = mpsc::channel(100);
        if state
            .mixer_tx
            .send(Event::Subscribe(
                "6715a54c-f1bd-55ea-9963-dd0f138c59d6".parse().unwrap(),
                tx,
            ))
            .await
            .is_err()
        {
            error!("failed to subscribe to mixer");
            state.shutdown.force_shutdown().await;
            return;
        }

        loop {
            tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok(event)) => forward_event(event, &mut state.mixer_tx).await,
                    None | Some(Err(MessageError::Socket)) => return,
                    _ => continue,
                },
                update = subscription.recv() => match update {
                    Some(event) => info!("received updated universe: {:?}", event),
                    None => {
                        error!("mixer channel closed unexpectedly");
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

async fn forward_event(event: Event, mixer_tx: &mut mpsc::Sender<Event>) {
    if mixer_tx.send(event).await.is_err() {
        error!("failed to send event to mixer");
    };
}
