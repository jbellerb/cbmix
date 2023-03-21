mod config;

use crate::shutdown;
pub use config::Config;

use axum::{
    http::StatusCode,
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
    config: Config,
    shutdown: shutdown::Receiver,
}

impl Interface {
    pub fn new(config: Config, shutdown: shutdown::Receiver) -> Self {
        Self { config, shutdown }
    }

    pub async fn serve(mut self) {
        let routes = Router::new().route("/ping", get(ping_handler));

        let app = routes.layer(
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

#[axum::debug_handler(state = Interface)]
pub async fn ping_handler() -> StatusCode {
    StatusCode::OK
}
