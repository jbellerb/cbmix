pub mod config;

use std::env::var;
use std::process::exit;

use config::{Config, InputConfig, NodeConfig, OutputConfig};

use anyhow::{anyhow, Error};
use cbmix_admin::Admin;
use cbmix_common::shutdown;
use cbmix_dmx::Dmx;
use cbmix_graph::{Graph, GraphHandle, Node, NAMESPACE_SCENE};
use directories::ProjectDirs;
use tokio::{
    runtime::Runtime,
    signal::unix::{signal, SignalKind},
    time::timeout,
};
use tracing::{debug, error, info, info_span, instrument::Instrument, warn};
use uuid::Uuid;

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

        let graph = Graph::new(shutdown.subscribe());

        let mut dmx = match Dmx::new(graph.handle(), shutdown.subscribe()).await {
            Ok(dmx) => dmx,
            Err(e) => {
                eprintln!("Error setting up DMX connections\n{}", e);
                exit(1);
            }
        };

        let admin = Admin::new(config.admin.clone(), graph.handle(), shutdown.subscribe());

        let init_handle = graph.handle();

        tokio::spawn(graph.serve().instrument(info_span!("graph")));

        match register_nodes(&config, init_handle, &mut dmx).await {
            Ok(()) => {
                tokio::spawn(dmx.serve().instrument(info_span!("dmx")));
                tokio::spawn(admin.serve().instrument(info_span!("admin")));
            }
            Err(e) => {
                error!("failed to register nodes from config file: {}", e);
                shutdown.subscribe().force_shutdown().await;
                drop(dmx);
                drop(admin);
            }
        }

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

async fn register_nodes(config: &Config, graph: GraphHandle, dmx: &mut Dmx) -> Result<(), Error> {
    for (id, InputConfig { universe, .. }) in &config.input {
        dmx.add_input(*universe, Uuid::new_v5(&NAMESPACE_SCENE, id.as_bytes()))
            .await?;
    }

    for (id, node) in &config.node {
        let id = Uuid::new_v5(&NAMESPACE_SCENE, id.as_bytes());
        let node = match node {
            NodeConfig::Static { channels } => Node::Input {
                channels: channels.clone(),
            },
            NodeConfig::Add { a, b } => {
                let a = a
                    .as_ref()
                    .map(|s| Uuid::new_v5(&NAMESPACE_SCENE, s.as_bytes()));
                let b = b
                    .as_ref()
                    .map(|s| Uuid::new_v5(&NAMESPACE_SCENE, s.as_bytes()));
                Node::Add { a, b }
            }
            NodeConfig::Multiply { a, b } => {
                let a = a
                    .as_ref()
                    .map(|s| Uuid::new_v5(&NAMESPACE_SCENE, s.as_bytes()));
                let b = b
                    .as_ref()
                    .map(|s| Uuid::new_v5(&NAMESPACE_SCENE, s.as_bytes()));
                Node::Multiply { a, b }
            }
            NodeConfig::Rewire { input, map } => {
                let input = input
                    .as_ref()
                    .map(|s| Uuid::new_v5(&NAMESPACE_SCENE, s.as_bytes()));
                Node::Rewire {
                    input,
                    map: Box::new(
                        (&map[..])
                            .try_into()
                            .map_err(|_| anyhow!("rewire map was not 512 long"))?,
                    ),
                }
            }
        };

        graph.insert(id, node).await?;
    }

    for (_, OutputConfig { universe, from }) in config.output.iter() {
        dmx.add_output(*universe, Uuid::new_v5(&NAMESPACE_SCENE, from.as_bytes()))
            .await?;
    }

    Ok(())
}
