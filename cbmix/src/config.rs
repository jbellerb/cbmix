use std::fmt;
use std::fs::read_to_string;
use std::marker::PhantomData;
use std::path::Path;
use std::time::Duration;

use cbmix_admin::config::AdminConfig;
use ola::DmxBuffer;
use regex::Regex;
use serde::{
    de::{Deserializer, Error as DeError, MapAccess, Unexpected, Visitor},
    Deserialize,
};
use thiserror::Error;

pub const DEFAULT_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(30);

#[derive(Error, Clone, Debug)]
pub enum Error {
    #[error("Error loading config: {0}")]
    LoadError(#[from] toml::de::Error),
}

pub type PairList<K, V> = Vec<(K, V)>;

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default, deserialize_with = "deserialize_pair_list")]
    pub input: PairList<String, InputConfig>,
    #[serde(deserialize_with = "deserialize_pair_list")]
    pub output: PairList<String, OutputConfig>,
    #[serde(deserialize_with = "deserialize_pair_list")]
    pub node: PairList<String, NodeConfig>,
    #[serde(
        default = "default_shutdown_grace_period",
        deserialize_with = "deserialize_duration"
    )]
    pub shutdown_grace_period: Duration,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InputConfig {
    _name: String,
    _universe: u32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub universe: u32,
    pub from: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NodeConfig {
    Static {
        #[serde(deserialize_with = "deserialize_buffer")]
        channels: DmxBuffer,
    },
    Add {
        a: Option<String>,
        b: Option<String>,
    },
    Multiply {
        a: Option<String>,
        b: Option<String>,
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
            admin: Default::default(),
            input: Default::default(),
            output: Default::default(),
            node: Default::default(),
            shutdown_grace_period: default_shutdown_grace_period(),
        }
    }
}

fn default_shutdown_grace_period() -> Duration {
    DEFAULT_SHUTDOWN_GRACE_PERIOD
}

pub fn deserialize_pair_list<'de, D, K, V>(deserializer: D) -> Result<PairList<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de>,
    V: Deserialize<'de>,
{
    struct PairVisitor<K, V> {
        m: PhantomData<fn() -> PairList<K, V>>,
    }

    impl<'de, K, V> Visitor<'de> for PairVisitor<K, V>
    where
        K: Deserialize<'de>,
        V: Deserialize<'de>,
    {
        type Value = PairList<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a list of key-value pairs")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut list = Vec::with_capacity(access.size_hint().unwrap_or(0));
            while let Some((key, value)) = access.next_entry()? {
                list.push((key, value));
            }

            Ok(list)
        }
    }

    deserializer.deserialize_map(PairVisitor { m: PhantomData })
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
            b'A'..=b'F' => *digit -= b'A' - 10,
            b'a'..=b'f' => *digit -= b'a' - 10,
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
