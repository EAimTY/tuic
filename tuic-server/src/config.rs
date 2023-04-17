use crate::utils::CongestionControl;
use lexopt::{Arg, Error as ArgumentError, Parser};
use log::LevelFilter;
use serde::{de::Error as DeError, Deserialize, Deserializer};
use serde_json::Error as SerdeError;
use std::{
    collections::HashMap, env::ArgsOs, fmt::Display, fs::File, io::Error as IoError,
    net::SocketAddr, path::PathBuf, str::FromStr, time::Duration,
};
use thiserror::Error;
use uuid::Uuid;

const HELP_MSG: &str = r#"
Usage tuic-server [arguments]

Arguments:
    -c, --config <path>     Path to the config file (required)
    -v, --version           Print the version
    -h, --help              Print this help message
"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: SocketAddr,
    #[serde(deserialize_with = "deserialize_users")]
    pub users: HashMap<Uuid, String>,
    pub certificate: PathBuf,
    pub private_key: PathBuf,
    #[serde(
        default = "default::congestion_control",
        deserialize_with = "deserialize_from_str"
    )]
    pub congestion_control: CongestionControl,
    #[serde(default = "default::alpn")]
    pub alpn: Vec<String>,
    #[serde(default = "default::udp_relay_ipv6")]
    pub udp_relay_ipv6: bool,
    #[serde(default = "default::zero_rtt_handshake")]
    pub zero_rtt_handshake: bool,
    pub dual_stack: Option<bool>,
    #[serde(
        default = "default::auth_timeout",
        deserialize_with = "deserialize_duration"
    )]
    pub auth_timeout: Duration,
    #[serde(
        default = "default::max_idle_time",
        deserialize_with = "deserialize_duration"
    )]
    pub max_idle_time: Duration,
    #[serde(default = "default::max_external_packet_size")]
    pub max_external_packet_size: usize,
    #[serde(
        default = "default::gc_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub gc_interval: Duration,
    #[serde(
        default = "default::gc_lifetime",
        deserialize_with = "deserialize_duration"
    )]
    pub gc_lifetime: Duration,
    #[serde(default = "default::log_level")]
    pub log_level: LevelFilter,
}

impl Config {
    pub fn parse(args: ArgsOs) -> Result<Self, ConfigError> {
        let mut parser = Parser::from_iter(args);
        let mut path = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Short('c') | Arg::Long("config") => {
                    if path.is_none() {
                        path = Some(parser.value()?);
                    } else {
                        return Err(ConfigError::Argument(arg.unexpected()));
                    }
                }
                Arg::Short('v') | Arg::Long("version") => {
                    return Err(ConfigError::Version(env!("CARGO_PKG_VERSION")))
                }
                Arg::Short('h') | Arg::Long("help") => return Err(ConfigError::Help(HELP_MSG)),
                _ => return Err(ConfigError::Argument(arg.unexpected())),
            }
        }

        if path.is_none() {
            return Err(ConfigError::NoConfig);
        }

        let file = File::open(path.unwrap())?;
        Ok(serde_json::from_reader(file)?)
    }
}

mod default {
    use crate::utils::CongestionControl;
    use log::LevelFilter;
    use std::time::Duration;

    pub fn congestion_control() -> CongestionControl {
        CongestionControl::Cubic
    }

    pub fn alpn() -> Vec<String> {
        Vec::new()
    }

    pub fn udp_relay_ipv6() -> bool {
        true
    }

    pub fn zero_rtt_handshake() -> bool {
        false
    }

    pub fn auth_timeout() -> Duration {
        Duration::from_secs(10)
    }

    pub fn max_idle_time() -> Duration {
        Duration::from_secs(15)
    }

    pub fn max_external_packet_size() -> usize {
        1500
    }

    pub fn gc_interval() -> Duration {
        Duration::from_secs(3)
    }

    pub fn gc_lifetime() -> Duration {
        Duration::from_secs(15)
    }

    pub fn log_level() -> LevelFilter {
        LevelFilter::Warn
    }
}

pub fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    <T as FromStr>::Err: Display,
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(&s).map_err(DeError::custom)
}

pub fn deserialize_users<'de, D>(deserializer: D) -> Result<HashMap<Uuid, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let map = HashMap::<Uuid, String>::deserialize(deserializer)?;

    if map.is_empty() {
        return Err(DeError::custom("users cannot be empty"));
    }

    Ok(map)
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let mut num = Vec::with_capacity(8);
    let mut chars = Vec::with_capacity(2);
    let mut expected_unit = false;
    for c in s.chars() {
        if !expected_unit && c.is_numeric() {
            num.push(c);
            continue;
        }

        if !expected_unit && c.is_ascii() {
            expected_unit = true;
        }
        chars.push(c);
    }

    let n: u64 = num
        .into_iter()
        .collect::<String>()
        .parse()
        .map_err(|e| format!("invalid value: {}, reason {}", &s, e))?;

    match chars.into_iter().collect::<String>().as_str() {
        "" => Ok(Duration::from_millis(n)),
        "s" => Ok(Duration::from_secs(n)),
        "ms" => Ok(Duration::from_millis(n)),
        _ => Err(format!("invalid value: {}, expected 10s or 10ms", &s)),
    }
}

#[test]
fn test_parseduration() {
    // test parse
    assert_eq!(parse_duration("100s"), Ok(Duration::from_secs(100)));
    assert_eq!(parse_duration("100ms"), Ok(Duration::from_millis(100)));

    // test default unit
    assert_eq!(parse_duration("10000"), Ok(Duration::from_millis(10000)));

    // test invalid data
    assert!(parse_duration("").is_err());
    assert!(parse_duration("1ms100").is_err());
    assert!(parse_duration("ms").is_err());
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    parse_duration(&s).map_err(DeError::custom)
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Argument(#[from] ArgumentError),
    #[error("no config file specified")]
    NoConfig,
    #[error("{0}")]
    Version(&'static str),
    #[error("{0}")]
    Help(&'static str),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Serde(#[from] SerdeError),
}
