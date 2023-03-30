use std::net::SocketAddr;

use serde::Deserialize;

pub const DEFAULT_LISTEN_ADDR: &str = "[::0]:8080";

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
        }
    }
}

fn default_listen_addr() -> SocketAddr {
    DEFAULT_LISTEN_ADDR.parse().unwrap()
}
