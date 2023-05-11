mod channel;
pub mod config;

use std::collections::HashSet;

use channel::{next, send, Error as ChannelError};
use config::AdminConfig;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use cbmix_admin_proto::{
    error_message, event::Event, GraphServiceRequest, GraphServiceResponse, NodeId,
    SubscriptionUpdateEvent,
};
use cbmix_common::shutdown;
use cbmix_graph::{GraphHandle, GraphUpdate};
use thiserror::Error;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, trace, warn, Level};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to subscribe to graph node")]
    Subscribe,
    #[error("Failed to unsubscribe from graph node")]
    Unsubscribe,
    #[error("Failed to get node")]
    Get,
    #[error("Failed to get nodes")]
    List,
    #[error("Failed to update node")]
    Update,
    #[error("Failed to remove node")]
    Remove,
    #[error("DMX universe must be 512 channels")]
    Channels(#[from] ola::TryFromBufferError),
    #[error("Unable to parse UUID")]
    Uuid(#[from] uuid::Error),
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
        let mut subscriptions = HashSet::new();

        loop {
            tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok((seq, request))) => {
                        let message = match handle_request(
                            request,
                            &mut state.graph,
                            &subscriber,
                            &mut subscriptions,
                        )
                        .await
                        {
                            Ok(res) => res.to_message(seq),
                            Err(e) => error_message(seq, e.to_string()),
                        };

                        let _ = send(&mut socket, message).await;
                    }
                    None | Some(Err(ChannelError::Socket)) => {
                        for subscription in subscriptions.iter() {
                            if let Err(e) = state.graph.unsubscribe(*subscription).await {
                                warn!("error removing subscription {}: {}", subscription, e);
                            }
                        }

                        return
                    }
                    _ => continue,
                },
                update = subscription.recv() => match update {
                    Some(update) => match update {
                        GraphUpdate::Update { id, channels } => {
                            trace!("received updated universe: {} -> {:?}", id, channels);
                            let message = SubscriptionUpdateEvent {
                                id: Some(NodeId { id: id.to_string() }),
                                channels: channels.into(),
                            }
                            .to_message();

                            let _ = send(&mut socket, message).await;
                        }
                        GraphUpdate::Closed { id } => info!("subscription closed: {}", id),
                    },
                    None => {
                        error!("graph channel closed unexpectedly");
                        break;
                    }
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
    subscriber: &mpsc::Sender<GraphUpdate>,
    subscriptions: &mut HashSet<Uuid>,
) -> Result<GraphServiceResponse, Error> {
    match request {
        GraphServiceRequest::Subscribe(id) => {
            let id = graph.subscribe(id, subscriber.clone()).await.map_err(|e| {
                error!("failed to subscribe: {}", e);
                Error::Subscribe
            })?;

            subscriptions.insert(id);
            Ok(GraphServiceResponse::Subscribe(id))
        }
        GraphServiceRequest::Unsubscribe(id) => {
            graph.unsubscribe(id).await.map_err(|e| {
                error!("failed to unsubscribe: {}", e);
                Error::Unsubscribe
            })?;

            subscriptions.remove(&id);
            Ok(GraphServiceResponse::Unsubscribe)
        }
        GraphServiceRequest::GetNode(id) => {
            let node = graph.get(id).await.map_err(|e| {
                error!("failed to get node {}: {}", id, e);
                Error::Get
            })?;

            Ok(GraphServiceResponse::GetNode(id, node))
        }
        GraphServiceRequest::GetNodes => {
            let nodes = graph.list().await.map_err(|e| {
                error!("failed to get nodes: {}", e);
                Error::List
            })?;

            Ok(GraphServiceResponse::GetNodes(nodes))
        }
        GraphServiceRequest::UpdateNode(id, body) => {
            let id = id.unwrap_or_else(Uuid::new_v4);
            graph.insert(id, body).await.map_err(|e| {
                error!("failed to update node {}: {}", id, e);
                Error::Update
            })?;

            Ok(GraphServiceResponse::UpdateNode(id))
        }
        GraphServiceRequest::RemoveNode(id) => {
            graph.remove(id).await.map_err(|e| {
                error!("failed to remove node {}: {}", id, e);
                Error::Remove
            })?;

            Ok(GraphServiceResponse::RemoveNode)
        }
    }
}
