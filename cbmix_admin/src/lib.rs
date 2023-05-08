pub mod config;
mod message;

use std::collections::HashMap;

use config::AdminConfig;
use message::{next, send, Error as MessageError};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use cbmix_admin_proto::{GraphServiceRequest, GraphServiceResponse, NodeId, OutputUpdateEvent};
use cbmix_common::shutdown;
use cbmix_graph::{GraphHandle, Index};
use ola::DmxBuffer;
use thiserror::Error;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, Level};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to subscribe to graph node")]
    Subscribe,
    #[error("Failed to unsubscribe from graph node")]
    Unsubscribe,
}

#[derive(Clone, Debug)]
pub struct Admin {
    config: AdminConfig,
    graph: GraphHandle,
    shutdown: shutdown::Receiver,
}

#[derive(Clone, Debug)]
struct ServerState {
    graph: GraphHandle,
    shutdown: shutdown::Receiver,
}

impl Admin {
    pub fn new(config: AdminConfig, graph: GraphHandle, shutdown: shutdown::Receiver) -> Self {
        Self {
            config,
            graph,
            shutdown,
        }
    }

    pub async fn serve(mut self) {
        let routes = Router::new().route("/api/ws", get(ws_handler));
        let state = ServerState {
            graph: self.graph,
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
        let (subscriber, mut subscription) = mpsc::channel(100);
        let mut subscriptions = HashMap::new();

        loop {
            tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok((seq, request))) => {
                        let response = handle_request(
                            request,
                            &mut state.graph,
                            &subscriber,
                            &mut subscriptions,
                        ).await;
                        let _ = send(&mut socket, seq, response).await;
                    },
                    None | Some(Err(MessageError::Socket)) => return,
                    _ => continue,
                },
                update = subscription.recv() => match update {
                    Some((id, channels)) => info!("received updated universe: {} -> {:?}", id, channels),
                    None => {
                        error!("graph channel closed unexpectedly");
                        break
                    },
                },
                _ = state.shutdown.recv() => break,
            }
        }

        if let Err(e) = socket.close().await {
            error!("error closing socket: {}", e);
        }
    })
}

async fn handle_request(
    request: GraphServiceRequest,
    graph: &mut GraphHandle,
    subscriber: &mpsc::Sender<(Uuid, DmxBuffer)>,
    subscriptions: &mut HashMap<Uuid, Index>,
) -> Result<GraphServiceResponse, Error> {
    match request {
        GraphServiceRequest::Subscribe(id) => {
            let idx = graph
                .subscribe(id, subscriber.clone())
                .await
                .map_err(|_| Error::Subscribe)?;
            subscriptions.insert(id, idx);

            Ok(GraphServiceResponse::Subscribe(OutputUpdateEvent {
                id: Some(NodeId { id: id.to_string() }),
                channels: Vec::new(),
            }))
        }
        GraphServiceRequest::Unsubscribe(id) => {
            if let Some(idx) = subscriptions.get(&id) {
                graph
                    .unsubscribe(id, *idx)
                    .await
                    .map_err(|_| Error::Unsubscribe)?;
            }

            Ok(GraphServiceResponse::Unsubscribe)
        }
        _ => unimplemented!("todo"),
    }
}
