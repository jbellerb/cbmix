pub mod config;
pub mod dmx;
pub mod interface;
pub mod mix;
pub mod scene;
pub mod shutdown;

pub use cbmix_admin_proto as proto;

use std::env::var;
use std::process::exit;

use config::Config;
use dmx::Dmx;
use interface::Interface;
use mix::Mixer;
use scene::SceneGraph;

use directories::ProjectDirs;
use tokio::{
    runtime::Runtime,
    signal::unix::{signal, SignalKind},
    time::timeout,
};
use tracing::{debug, error, info, info_span, instrument::Instrument, warn};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let dirs = ProjectDirs::from("", "", "cbmix").expect("determine program directories");
    let config = match Config::try_from_file(&dirs.config_dir().join("config.toml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    };

    build_runtime().block_on(async move {
        let mut shutdown = shutdown::Sender::new();

        let graph = match SceneGraph::from_config(&config) {
            Ok(graph) => graph,
            Err(e) => {
                eprintln!("Error building scene graph\n{}", e);
                exit(1);
            }
        };
        let mixer = Mixer::new(graph, shutdown.subscribe());

        let dmx = match Dmx::new(config.output, mixer.sender(), shutdown.subscribe()).await {
            Ok(dmx) => dmx,
            Err(e) => {
                eprintln!("Error setting up DMX connections\n{}", e);
                exit(1);
            }
        };

        let interface = Interface::new(config.interface, mixer.sender(), shutdown.subscribe());

        tokio::spawn(mixer.serve().instrument(info_span!("mixer")));
        tokio::spawn(dmx.serve().instrument(info_span!("dmx")));
        tokio::spawn(interface.serve().instrument(info_span!("interface")));

        tokio::select! {
            _ = unix_signal(SignalKind::interrupt()) => {
                info!("received SIGINT, shutting down");
            },
            _ = unix_signal(SignalKind::terminate()) => {
                info!("received SIGTERM, shutting down");
            },
            _ = shutdown.recv() => {
                error!("shutting down due to unexpected error");
            },
        }

        match timeout(config.shutdown_grace_period, shutdown.shutdown()).await {
            Ok(()) => debug!("shutdown completed"),
            Err(_) => warn!(
                "graceful shutdown did not complete in {:?}, closing anyways",
                config.shutdown_grace_period
            ),
        }
    })
}

fn build_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build a multi-threaded tokio runtime")
}

async fn unix_signal(kind: SignalKind) {
    signal(kind)
        .expect("register a unix signal handler")
        .recv()
        .await;
}