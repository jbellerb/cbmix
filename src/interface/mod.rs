mod event;

use crate::config::InterfaceConfig;
use crate::shutdown;
use event::{next, Error as EventError, Event};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, Level};

pub struct Interface {
    config: InterfaceConfig,
    shutdown: shutdown::Receiver,
}

#[derive(Clone)]
struct ServerState {
    shutdown: shutdown::Receiver,
}

impl Interface {
    pub fn new(config: InterfaceConfig, shutdown: shutdown::Receiver) -> Self {
        Self { config, shutdown }
    }

    pub async fn serve(mut self) {
        let routes = Router::new().route("/api/ws", get(ws_handler));
        let state = ServerState {
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
            let event = tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok(event)) => event,
                    None | Some(Err(EventError::Socket)) => return,
                    _ => continue,
                },
                _ = state.shutdown.recv() => {
                    if let Err(e) = socket.close().await {
                        error!("error closing socket: {}", e);
                    }

                    return
                }
            };

            match event {
                Event::SceneUpdate(event) => {
                    info!("recieved: {:?}", event);
                }
                Event::SetCurrentSceneRequest(event) => {
                    info!("recieved: {:?}", event);
                }
            }
        }
    })
}
