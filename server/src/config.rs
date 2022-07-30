use crate::certificate;
use getopts::{Fail, Options};
use log::{LevelFilter, ParseLevelError};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    IdleTimeout, ServerConfig, VarInt,
};
use rustls::{version::TLS13, Error as RustlsError, ServerConfig as RustlsServerConfig};
use serde::{de::Error as DeError, Deserialize, Deserializer};
use serde_json::Error as JsonError;
use std::{
    collections::HashSet,
    env::ArgsOs,
    fmt::Display,
    fs::File,
    io::Error as IoError,
    net::{AddrParseError, IpAddr, Ipv4Addr, SocketAddr},
    num::ParseIntError,
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

pub struct Config {
    pub server_config: ServerConfig,
    pub listen_addr: SocketAddr,
    pub token: HashSet<[u8; 32]>,
    pub authentication_timeout: Duration,
    pub max_udp_relay_packet_size: usize,
    pub log_level: LevelFilter,
}

impl Config {
    pub fn parse(args: ArgsOs) -> Result<Self, ConfigError> {
        let raw = RawConfig::parse(args)?;

        let server_config = {
            let cert_path = raw.certificate.unwrap();
            let certs = certificate::load_certificates(&cert_path)
                .map_err(|err| ConfigError::Io(cert_path, err))?;

            let priv_key_path = raw.private_key.unwrap();
            let priv_key = certificate::load_private_key(&priv_key_path)
                .map_err(|err| ConfigError::Io(priv_key_path, err))?;

            let mut crypto = RustlsServerConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&TLS13])
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(certs, priv_key)?;

            crypto.max_early_data_size = u32::MAX;
            crypto.alpn_protocols = raw.alpn.into_iter().map(|alpn| alpn.into_bytes()).collect();

            let mut config = ServerConfig::with_crypto(Arc::new(crypto));
            let transport = Arc::get_mut(&mut config.transport).unwrap();

            match raw.congestion_controller {
                CongestionController::Bbr => {
                    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
                }
                CongestionController::Cubic => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
                }
                CongestionController::NewReno => {
                    transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
                }
            }

            transport
                .max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(raw.max_idle_time))));

            config
        };

        let listen_addr = SocketAddr::from((raw.ip, raw.port.unwrap()));

        let token = raw
            .token
            .into_iter()
            .map(|token| *blake3::hash(&token.into_bytes()).as_bytes())
            .collect();

        let authentication_timeout = Duration::from_secs(raw.authentication_timeout);
        let max_udp_relay_packet_size = raw.max_udp_relay_packet_size;
        let log_level = raw.log_level;

        Ok(Self {
            server_config,
            listen_addr,
            token,
            authentication_timeout,
            max_udp_relay_packet_size,
            log_level,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    port: Option<u16>,
    token: Vec<String>,
    certificate: Option<String>,
    private_key: Option<String>,

    #[serde(default = "default::ip")]
    ip: IpAddr,

    #[serde(
        default = "default::congestion_controller",
        deserialize_with = "deserialize_from_str"
    )]
    congestion_controller: CongestionController,

    #[serde(default = "default::max_idle_time")]
    max_idle_time: u32,

    #[serde(default = "default::authentication_timeout")]
    authentication_timeout: u64,

    #[serde(default = "default::alpn")]
    alpn: Vec<String>,

    #[serde(default = "default::max_udp_relay_packet_size")]
    max_udp_relay_packet_size: usize,

    #[serde(default = "default::log_level")]
    log_level: LevelFilter,
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            port: None,
            token: Vec::new(),
            certificate: None,
            private_key: None,
            ip: default::ip(),
            congestion_controller: default::congestion_controller(),
            max_idle_time: default::max_idle_time(),
            authentication_timeout: default::authentication_timeout(),
            alpn: default::alpn(),
            max_udp_relay_packet_size: default::max_udp_relay_packet_size(),
            log_level: default::log_level(),
        }
    }
}

impl RawConfig {
    fn parse(args: ArgsOs) -> Result<Self, ConfigError> {
        let mut opts = Options::new();

        opts.optopt(
            "c",
            "config",
            "Read configuration from a file. Note that command line arguments will override the configuration file",
            "CONFIG_FILE",
        );

        opts.optopt("", "port", "Set the server listening port", "SERVER_PORT");

        opts.optmulti(
            "",
            "token",
            "Set the token for TUIC authentication. This option can be used multiple times to set multiple tokens.",
            "TOKEN",
        );

        opts.optopt(
            "",
            "certificate",
            "Set the X.509 certificate. This must be an end-entity certificate",
            "CERTIFICATE",
        );

        opts.optopt(
            "",
            "private-key",
            "Set the certificate private key",
            "PRIVATE_KEY",
        );

        opts.optopt(
            "",
            "ip",
            "Set the server listening IP. Default: 0.0.0.0",
            "IP",
        );

        opts.optopt(
            "",
            "congestion-controller",
            r#"Set the congestion control algorithm. Available: "cubic", "new_reno", "bbr". Default: "cubic""#,
            "CONGESTION_CONTROLLER",
        );

        opts.optopt(
            "",
            "max-idle-time",
            "Set the maximum idle time for QUIC connections, in milliseconds. Default: 15000",
            "MAX_IDLE_TIME",
        );

        opts.optopt(
            "",
            "authentication-timeout",
            "Set the maximum time allowed between a QUIC connection established and the TUIC authentication packet received, in milliseconds. Default: 1000",
            "AUTHENTICATION_TIMEOUT",
        );

        opts.optmulti(
            "",
            "alpn",
            "Set ALPN protocols that the server accepts. This option can be used multiple times to set multiple ALPN protocols. If not set, the server will not check ALPN at all",
            "ALPN_PROTOCOL",
        );

        opts.optopt(
            "",
            "max-udp-relay-packet-size",
            "UDP relay mode QUIC can transmit UDP packets larger than the MTU. Set this to a higher value allows outbound to receive larger UDP packet. Default: 1500",
            "MAX_UDP_RELAY_PACKET_SIZE",
        );

        opts.optopt(
            "",
            "log-level",
            r#"Set the log level. Available: "off", "error", "warn", "info", "debug", "trace". Default: "info""#,
            "LOG_LEVEL",
        );

        opts.optflag("v", "version", "Print the version");
        opts.optflag("h", "help", "Print this help menu");

        let matches = opts.parse(args.skip(1))?;

        if matches.opt_present("help") {
            return Err(ConfigError::Help(opts.usage(env!("CARGO_PKG_NAME"))));
        }

        if matches.opt_present("version") {
            return Err(ConfigError::Version(env!("CARGO_PKG_VERSION")));
        }

        if !matches.free.is_empty() {
            return Err(ConfigError::UnexpectedArguments(matches.free.join(", ")));
        }

        let port = matches.opt_str("port").map(|port| port.parse());
        let token = matches.opt_strs("token");
        let certificate = matches.opt_str("certificate");
        let private_key = matches.opt_str("private-key");

        let mut raw = if let Some(path) = matches.opt_str("config") {
            let mut raw = RawConfig::from_file(path)?;

            raw.port = Some(
                port.transpose()?
                    .or(raw.port)
                    .ok_or(ConfigError::MissingOption("port"))?,
            );

            if !token.is_empty() {
                raw.token = token;
            } else if raw.token.is_empty() {
                return Err(ConfigError::MissingOption("token"));
            }

            raw.certificate = Some(
                certificate
                    .or(raw.certificate)
                    .ok_or(ConfigError::MissingOption("certificate"))?,
            );

            raw.private_key = Some(
                private_key
                    .or(raw.private_key)
                    .ok_or(ConfigError::MissingOption("private key"))?,
            );

            raw
        } else {
            RawConfig {
                port: Some(port.ok_or(ConfigError::MissingOption("port"))??),
                token: (!token.is_empty())
                    .then(|| token)
                    .ok_or(ConfigError::MissingOption("token"))?,
                certificate: Some(certificate.ok_or(ConfigError::MissingOption("certificate"))?),
                private_key: Some(private_key.ok_or(ConfigError::MissingOption("private key"))?),
                ..Default::default()
            }
        };

        if let Some(ip) = matches.opt_str("ip") {
            raw.ip = ip.parse()?;
        };

        if let Some(cgstn_ctrl) = matches.opt_str("congestion-controller") {
            raw.congestion_controller = cgstn_ctrl.parse()?;
        };

        if let Some(timeout) = matches.opt_str("max-idle-time") {
            raw.max_idle_time = timeout.parse()?;
        };

        if let Some(timeout) = matches.opt_str("authentication-timeout") {
            raw.authentication_timeout = timeout.parse()?;
        };

        if let Some(size) = matches.opt_str("max-udp-relay-packet-size") {
            raw.max_udp_relay_packet_size = size.parse()?;
        };

        let alpn = matches.opt_strs("alpn");

        if !alpn.is_empty() {
            raw.alpn = alpn;
        }

        if let Some(log_level) = matches.opt_str("log-level") {
            raw.log_level = log_level.parse()?;
        };

        Ok(raw)
    }

    fn from_file(path: String) -> Result<Self, ConfigError> {
        let file = File::open(&path).map_err(|err| ConfigError::Io(path, err))?;
        let raw = serde_json::from_reader(file)?;
        Ok(raw)
    }
}

enum CongestionController {
    Cubic,
    NewReno,
    Bbr,
}

impl FromStr for CongestionController {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("cubic") {
            Ok(CongestionController::Cubic)
        } else if s.eq_ignore_ascii_case("new_reno") || s.eq_ignore_ascii_case("newreno") {
            Ok(CongestionController::NewReno)
        } else if s.eq_ignore_ascii_case("bbr") {
            Ok(CongestionController::Bbr)
        } else {
            Err(ConfigError::InvalidCongestionController)
        }
    }
}

fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    <T as FromStr>::Err: Display,
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(&s).map_err(DeError::custom)
}

mod default {
    use super::*;

    pub(super) fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    }

    pub(super) const fn congestion_controller() -> CongestionController {
        CongestionController::Cubic
    }

    pub(super) const fn max_idle_time() -> u32 {
        15000
    }

    pub(super) const fn authentication_timeout() -> u64 {
        1000
    }

    pub(super) const fn alpn() -> Vec<String> {
        Vec::new()
    }

    pub(super) const fn max_udp_relay_packet_size() -> usize {
        1500
    }

    pub(super) const fn log_level() -> LevelFilter {
        LevelFilter::Info
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("{0}")]
    Help(String),
    #[error("{0}")]
    Version(&'static str),
    #[error("Failed to read '{0}': {1}")]
    Io(String, #[source] IoError),
    #[error("Failed to parse the config file: {0}")]
    ParseConfigJson(#[from] JsonError),
    #[error(transparent)]
    ParseArgument(#[from] Fail),
    #[error("Unexpected arguments: {0}")]
    UnexpectedArguments(String),
    #[error("Missing option: {0}")]
    MissingOption(&'static str),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    ParseAddr(#[from] AddrParseError),
    #[error("Invalid congestion controller")]
    InvalidCongestionController,
    #[error(transparent)]
    ParseLogLevel(#[from] ParseLevelError),
    #[error("Failed to load certificate / private key: {0}")]
    Rustls(#[from] RustlsError),
}
