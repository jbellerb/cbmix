pub mod config;
mod message;

use config::AdminConfig;
use message::{next, send, Error as MessageError};

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, Router},
    Server,
};
use cbmix_admin_proto::{
    node::Body, GraphServiceRequest, GraphServiceResponse, InputNode, NodeId, OutputUpdateEvent,
};
use cbmix_common::shutdown;
use cbmix_graph::{GraphHandle, GraphUpdate, Node};
use thiserror::Error;
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, Level};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to subscribe to graph node")]
    Subscribe,
    #[error("Failed to unsubscribe from graph node")]
    Unsubscribe,
    #[error("Node not present with given id")]
    Id,
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

        loop {
            tokio::select! {
                message = next(&mut socket) => match message {
                    Some(Ok((seq, request))) => {
                        let response = handle_request(
                            request,
                            &mut state.graph,
                            &subscriber,
                        ).await;
                        let _ = send(&mut socket, seq, response).await;
                    },
                    None | Some(Err(MessageError::Socket)) => return,
                    _ => continue,
                },
                update = subscription.recv() => match update {
                    Some(update) => info!("received updated universe: {:?}", update),
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
    subscriber: &mpsc::Sender<GraphUpdate>,
) -> Result<GraphServiceResponse, Error> {
    match request {
        GraphServiceRequest::Subscribe(id) => {
            let id = graph.subscribe(id, subscriber.clone()).await.map_err(|e| {
                error!("failed to subscribe: {}", e);
                Error::Subscribe
            })?;

            Ok(GraphServiceResponse::Subscribe(OutputUpdateEvent {
                id: Some(NodeId { id: id.to_string() }),
                channels: vec![0; 512],
            }))
        }
        GraphServiceRequest::Unsubscribe(id) => {
            graph.unsubscribe(id).await.map_err(|e| {
                error!("failed to unsubscribe: {}", e);
                Error::Unsubscribe
            })?;

            Ok(GraphServiceResponse::Unsubscribe)
        }
        GraphServiceRequest::GetNode(_id) => unimplemented!("get_node"),
        GraphServiceRequest::GetNodes => unimplemented!("get_nodes"),
        GraphServiceRequest::UpdateNode(id, body) => {
            match body {
                Body::Input(InputNode { channels }) => {
                    graph
                        .insert(
                            id,
                            Node::Input {
                                channels: channels.try_into()?,
                            },
                        )
                        .await
                        .map_err(|_| Error::Id)?;
                }
            }

            Ok(GraphServiceResponse::UpdateNode(NodeId {
                id: id.to_string(),
            }))
        }
        GraphServiceRequest::RemoveNode(id) => {
            graph.remove(id).await.map_err(|_| Error::Id)?;

            Ok(GraphServiceResponse::RemoveNode)
        }
    }
}
