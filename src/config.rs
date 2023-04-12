use std::collections::BTreeMap;
use std::fs::read_to_string;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use crate::scene::NAMESPACE_SCENE;

use ola::DmxBuffer;
use regex::Regex;
use serde::{
    de::{Deserializer, Error as DeError, Unexpected},
    Deserialize,
};
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_LISTEN_ADDR: &str = "[::0]:8080";

pub const DEFAULT_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(30);

#[derive(Error, Clone, Debug)]
pub enum Error {
    #[error("Error loading config: {0}")]
    LoadError(#[from] toml::de::Error),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub interface: InterfaceConfig,

    #[serde(default, deserialize_with = "deserialize_ident_map")]
    pub input: BTreeMap<Uuid, InputConfig>,

    #[serde(default, deserialize_with = "deserialize_ident_map")]
    pub output: BTreeMap<Uuid, OutputConfig>,

    #[serde(
        default = "default_shutdown_grace_period",
        deserialize_with = "deserialize_duration"
    )]
    pub shutdown_grace_period: Duration,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InterfaceConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InputConfig {
    Static {
        name: String,
        #[serde(deserialize_with = "deserialize_buffer")]
        channels: DmxBuffer,
    },
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged, deny_unknown_fields)]
pub enum OutputConfig {
    Sacn {
        universe: u32,
        #[serde(deserialize_with = "deserialize_ident")]
        from: Uuid,
    },
}

impl Config {
    pub fn try_from_file(file: &Path) -> Result<Self, Error> {
        if file.exists() {
            let text = read_to_string(file).expect("read config file");
            Ok(toml::from_str(&text)?)
        } else {
            Ok(Default::default())
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interface: Default::default(),
            input: Default::default(),
            output: Default::default(),
            shutdown_grace_period: default_shutdown_grace_period(),
        }
    }
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
        }
    }
}

fn default_listen_addr() -> SocketAddr {
    DEFAULT_LISTEN_ADDR.parse().unwrap()
}

fn default_shutdown_grace_period() -> Duration {
    DEFAULT_SHUTDOWN_GRACE_PERIOD
}

pub fn deserialize_ident_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<Uuid, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    BTreeMap::<String, T>::deserialize(deserializer).map(|map| {
        map.into_iter()
            .map(|(k, v)| (Uuid::new_v5(&NAMESPACE_SCENE, k.as_bytes()), v))
            .collect()
    })
}

pub fn deserialize_ident<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer).map(|ident| Uuid::new_v5(&NAMESPACE_SCENE, ident.as_bytes()))
}

pub fn deserialize_buffer<'de, D>(deserializer: D) -> Result<DmxBuffer, D::Error>
where
    D: Deserializer<'de>,
{
    let mut hex = String::deserialize(deserializer)?;
    hex.retain(|c| !c.is_whitespace());
    if hex.as_bytes().len() != 1024 {
        return Err(D::Error::invalid_length(hex.len() / 2, &"512 DMX channels"));
    }

    let mut hex = hex.into_bytes();
    for digit in &mut hex {
        match digit {
            b'0'..=b'9' => *digit -= b'0',
            b'A'..=b'F' => *digit -= b'A' + 10,
            b'a'..=b'f' => *digit -= b'a' + 10,
            _ => {
                return Err(D::Error::invalid_value(
                    Unexpected::Char(*digit as char),
                    &"a hex character",
                ))
            }
        }
    }

    let mut bytes = Vec::with_capacity(512);
    for pair in hex.chunks_exact(2) {
        bytes.push(pair[0] << 4 | pair[1]);
    }

    Ok(bytes.try_into().expect("convert byte vector to DMX buffer"))
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let re = Regex::new(r"^\s*(\d+)\s*(s|m|h)?\s*$").expect("build duration parsing regex");
    let str = String::deserialize(deserializer)?;

    let caps = re.captures(&str).ok_or(D::Error::invalid_value(
        Unexpected::Str(&str),
        &"a number, optionally followed by s, m, or h",
    ))?;

    let value = (caps[1])
        .parse()
        .map_err(|_| D::Error::invalid_value(Unexpected::Str(&caps[1]), &"a number"))?;

    match caps.get(2).map(|m| m.as_str()) {
        Some("s") | None => Ok(Duration::from_secs(value)),
        Some("m") => Ok(Duration::from_secs(value * 60)),
        Some("h") => Ok(Duration::from_secs(value * 60 * 60)),
        Some(s) => Err(D::Error::invalid_value(Unexpected::Str(s), &"s, m, or h")),
    }
}
