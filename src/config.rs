use std::fs::read_to_string;
use std::path::Path;
use std::time::Duration;

use crate::interface;

use regex::Regex;
use serde::{
    de::{Deserializer, Error as DeError, Unexpected},
    Deserialize,
};
use thiserror::Error;

pub const DEFAULT_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(60);

#[derive(Error, Clone, Debug)]
pub enum Error {
    #[error("Error loading config: {0}")]
    LoadError(#[from] toml::de::Error),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub interface: interface::Config,

    #[serde(deserialize_with = "deserialize_duration")]
    #[serde(default = "default_shutdown_grace_period")]
    pub shutdown_grace_period: Duration,
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
            shutdown_grace_period: default_shutdown_grace_period(),
        }
    }
}

fn default_shutdown_grace_period() -> Duration {
    DEFAULT_SHUTDOWN_GRACE_PERIOD
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
