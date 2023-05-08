pub mod config;

use std::env::var;
use std::process::exit;

use config::{Config, InputConfig, OutputConfig};

use anyhow::Error;
use cbmix_admin::Admin;
use cbmix_common::shutdown;
use cbmix_dmx::Dmx;
use cbmix_graph::{Graph, GraphHandle};
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

        if register_nodes(&config, init_handle, &mut dmx).await.is_ok() {
            tokio::spawn(dmx.serve().instrument(info_span!("dmx")));
            tokio::spawn(admin.serve().instrument(info_span!("admin")));
        } else {
            shutdown.subscribe().force_shutdown().await;
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
    for (id, input) in &config.input {
        match input {
            InputConfig::Static { channels, .. } => {
                graph.create_input(*id, channels.clone()).await?;
            }
        }
    }

    for (id, output) in &config.output {
        match output {
            OutputConfig::Sacn { universe, from } => {
                graph.create_output(*id, *from).await?;

                dmx.add_universe(*universe, *id).await?;
            }
        }
    }

    Ok(())
}
