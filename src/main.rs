pub mod config;
pub mod interface;
pub mod shutdown;

use std::env::var;
use std::process::exit;

use config::Config;
use interface::Interface;

use directories::ProjectDirs;
use tokio::{
    runtime::Runtime,
    select,
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

        let interface = Interface::new(config.interface, shutdown.subscribe());

        tokio::spawn(interface.serve().instrument(info_span!("interface")));

        select! {
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
