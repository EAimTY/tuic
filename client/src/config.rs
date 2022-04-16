use crate::{
    certificate,
    relay::{ServerAddr, UdpMode},
    socks5::Authentication as Socks5Authentication,
};
use getopts::{Fail, Options};
use log::{LevelFilter, ParseLevelError};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ClientConfig, IdleTimeout, VarInt,
};
use rustls::{version::TLS13, Certificate, ClientConfig as RustlsClientConfig, RootCertStore};
use serde::{de::Error as DeError, Deserialize, Deserializer};
use serde_json::Error as JsonError;
use std::{
    env::ArgsOs,
    fmt::Display,
    fs::File,
    io::Error as IoError,
    net::{AddrParseError, IpAddr, Ipv4Addr, SocketAddr},
    num::ParseIntError,
    str::FromStr,
    sync::Arc,
};
use thiserror::Error;
use webpki::Error as WebpkiError;

pub struct Config {
    pub client_config: ClientConfig,
    pub server_addr: ServerAddr,
    pub token_digest: [u8; 32],
    pub udp_mode: UdpMode,
    pub heartbeat_interval: u64,
    pub ipv6_endpoint: bool,
    pub reduce_rtt: bool,
    pub local_addr: SocketAddr,
    pub socks5_authentication: Socks5Authentication,
    pub max_udp_packet_size: usize,
    pub log_level: LevelFilter,
}

impl Config {
    pub fn parse(args: ArgsOs) -> Result<Self, ConfigError> {
        let raw = RawConfig::parse(args)?;

        let client_config = {
            let mut roots = RootCertStore::empty();

            if let Some(path) = raw.relay.certificate {
                for cert in certificate::load_certificates(&path)
                    .map_err(|err| ConfigError::Io(path, err))?
                {
                    roots.add(&cert)?;
                }
            } else {
                for cert in rustls_native_certs::load_native_certs()
                    .map_err(|err| ConfigError::NativeCertificate(err))?
                {
                    roots.add(&Certificate(cert.0))?;
                }
            }

            let mut crypto = RustlsClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&TLS13])
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();

            crypto.alpn_protocols = raw
                .relay
                .alpn
                .into_iter()
                .map(|alpn| alpn.into_bytes())
                .collect();

            crypto.enable_sni = !raw.relay.disable_sni;

            let mut config = ClientConfig::new(Arc::new(crypto));
            let transport = Arc::get_mut(&mut config.transport).unwrap();

            match raw.relay.congestion_controller {
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

            transport.max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(
                raw.relay.max_idle_time,
            ))));

            config
        };

        let server_addr = {
            let name = raw.relay.server.unwrap();
            let port = raw.relay.port.unwrap();

            if let Some(ip) = raw.relay.ip {
                ServerAddr::SocketAddr {
                    server_addr: SocketAddr::new(ip, port),
                    server_name: name,
                }
            } else {
                ServerAddr::HostnameAddr {
                    hostname: name,
                    server_port: port,
                }
            }
        };

        let token_digest = *blake3::hash(&raw.relay.token.unwrap().into_bytes()).as_bytes();
        let udp_mode = raw.relay.udp_mode;
        let heartbeat_interval = raw.relay.heartbeat_interval;
        let ipv6_endpoint = raw.relay.ipv6_endpoint;
        let reduce_rtt = raw.relay.reduce_rtt;

        let local_addr = SocketAddr::from((raw.local.ip, raw.local.port.unwrap()));

        let socks5_authentication = match (raw.local.username, raw.local.password) {
            (None, None) => Socks5Authentication::None,
            (Some(username), Some(password)) => Socks5Authentication::Password {
                username: username.into_bytes(),
                password: password.into_bytes(),
            },
            _ => return Err(ConfigError::LocalAuthentication),
        };

        let max_udp_packet_size = raw.max_udp_packet_size;
        let log_level = raw.log_level;

        Ok(Self {
            client_config,
            server_addr,
            token_digest,
            udp_mode,
            heartbeat_interval,
            ipv6_endpoint,
            reduce_rtt,
            local_addr,
            socks5_authentication,
            max_udp_packet_size,
            log_level,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    relay: RawRelayConfig,
    local: RawLocalConfig,

    #[serde(default = "default::max_udp_packet_size")]
    max_udp_packet_size: usize,

    #[serde(default = "default::log_level")]
    log_level: LevelFilter,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRelayConfig {
    server: Option<String>,
    port: Option<u16>,
    token: Option<String>,
    ip: Option<IpAddr>,
    certificate: Option<String>,

    #[serde(
        default = "default::udp_mode",
        deserialize_with = "deserialize_from_str"
    )]
    udp_mode: UdpMode,

    #[serde(
        default = "default::congestion_controller",
        deserialize_with = "deserialize_from_str"
    )]
    congestion_controller: CongestionController,

    #[serde(default = "default::max_idle_time")]
    max_idle_time: u32,

    #[serde(default = "default::heartbeat_interval")]
    heartbeat_interval: u64,

    #[serde(default = "default::alpn")]
    alpn: Vec<String>,

    #[serde(default = "default::disable_sni")]
    disable_sni: bool,

    #[serde(default = "default::ipv6_endpoint")]
    ipv6_endpoint: bool,

    #[serde(default = "default::reduce_rtt")]
    reduce_rtt: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLocalConfig {
    port: Option<u16>,

    #[serde(default = "default::local_ip")]
    ip: IpAddr,

    username: Option<String>,
    password: Option<String>,
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            relay: RawRelayConfig::default(),
            local: RawLocalConfig::default(),
            max_udp_packet_size: default::max_udp_packet_size(),
            log_level: default::log_level(),
        }
    }
}

impl Default for RawRelayConfig {
    fn default() -> Self {
        Self {
            server: None,
            port: None,
            ip: None,
            token: None,
            certificate: None,
            udp_mode: default::udp_mode(),
            congestion_controller: default::congestion_controller(),
            max_idle_time: default::max_idle_time(),
            heartbeat_interval: default::heartbeat_interval(),
            alpn: default::alpn(),
            disable_sni: default::disable_sni(),
            ipv6_endpoint: default::ipv6_endpoint(),
            reduce_rtt: default::reduce_rtt(),
        }
    }
}

impl Default for RawLocalConfig {
    fn default() -> Self {
        Self {
            port: None,
            ip: default::local_ip(),
            username: None,
            password: None,
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

        opts.optopt(
            "",
            "server",
            "Set the server address. This address must be included in the certificate",
            "SERVER",
        );

        opts.optopt("", "server-port", "Set the server port", "SERVER_PORT");

        opts.optopt(
            "",
            "token",
            "Set the token for TUIC authentication",
            "TOKEN",
        );

        opts.optopt(
            "",
            "server-ip",
            "Set the server IP, for overwriting the DNS lookup result of the server address set in option 'server'",
            "SERVER_IP",
        );

        opts.optopt(
            "",
            "certificate",
            "Set the X.509 certificate for QUIC handshake. If not set, native CA roots will be trusted",
            "CERTIFICATE",
        );

        opts.optopt(
            "",
            "udp-mode",
            r#"Set the UDP relay mode. Available: "native", "quic". Default: "native""#,
            "UDP_MODE",
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
            "Set the maximum idle time for connections, in milliseconds. The true idle timeout is the minimum of this and the client's one. Default: 15000",
            "MAX_IDLE_TIME",
        );

        opts.optopt(
            "",
            "heartbeat-interval",
            "Set the heartbeat interval to ensures that the QUIC connection is not closed when there are relay tasks but no data transfer, in milliseconds. This value needs to be smaller than the maximum idle time of the server and client. Default: 10000",
            "HEARTBEAT_INTERVAL",
        );

        opts.optopt(
            "",
            "alpn",
            "Set ALPN protocols included in the TLS client hello. This option can be used multiple times to set multiple ALPN protocols. If not set, no ALPN extension will be sent",
            "ALPN_PROTOCOL",
        );

        opts.optflag(
            "",
            "disable-sni",
            "Not sending the Server Name Indication (SNI) extension during the client TLS handshake",
        );

        opts.optflag(
            "",
            "ipv6-endpoint",
            "Construct the endpoint from the IPv6 stack",
        );

        opts.optflag("", "reduce-rtt", "Enable 0-RTT QUIC handshake");

        opts.optopt(
            "",
            "local-port",
            "Set the listening port for the local socks5 server",
            "LOCAL_PORT",
        );

        opts.optopt(
            "",
            "local-ip",
            r#"Set the listening IP for the local socks5 server. Note that the sock5 server socket will be a dual-stack socket if it is IPv6. Default: "127.0.0.1""#,
            "LOCAL_IP",
        );

        opts.optopt(
            "",
            "local-username",
            "Set the username for the local socks5 server authentication",
            "LOCAL_USERNAME",
        );

        opts.optopt(
            "",
            "local-password",
            "Set the password for the local socks5 server authentication",
            "LOCAL_PASSWORD",
        );

        opts.optopt(
            "",
            "max-udp-packet-size",
            "Set the maximum UDP packet size, in bytes. Excess bytes may be discarded. Default: 1536",
            "MAX_UDP_PACKET_SIZE",
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

        let server = matches.opt_str("server");
        let server_port = matches.opt_str("server-port").map(|port| port.parse());
        let token = matches.opt_str("token");
        let local_port = matches.opt_str("local-port").map(|port| port.parse());

        let mut raw = if let Some(path) = matches.opt_str("config") {
            let mut raw = RawConfig::from_file(path)?;

            raw.relay.server = Some(
                server
                    .or(raw.relay.server)
                    .ok_or(ConfigError::MissingOption("server address"))?,
            );

            raw.relay.port = Some(
                server_port
                    .transpose()?
                    .or(raw.relay.port)
                    .ok_or(ConfigError::MissingOption("server port"))?,
            );

            raw.relay.token = Some(
                token
                    .or(raw.relay.token)
                    .ok_or(ConfigError::MissingOption("token"))?,
            );

            raw.local.port = Some(
                local_port
                    .transpose()?
                    .or(raw.local.port)
                    .ok_or(ConfigError::MissingOption("local port"))?,
            );

            raw
        } else {
            let relay = RawRelayConfig {
                server: Some(server.ok_or(ConfigError::MissingOption("server address"))?),
                port: Some(server_port.ok_or(ConfigError::MissingOption("server port"))??),
                token: Some(token.ok_or(ConfigError::MissingOption("token"))?),
                ..Default::default()
            };

            let local = RawLocalConfig {
                port: Some(local_port.ok_or(ConfigError::MissingOption("local port"))??),
                ..Default::default()
            };

            RawConfig {
                relay,
                local,
                ..Default::default()
            }
        };

        if let Some(ip) = matches.opt_str("server-ip") {
            raw.relay.ip = Some(ip.parse()?);
        };

        raw.relay.certificate = matches.opt_str("certificate").or(raw.relay.certificate);

        if let Some(mode) = matches.opt_str("udp-mode") {
            raw.relay.udp_mode = mode.parse()?;
        };

        if let Some(cgstn_ctrl) = matches.opt_str("congestion-controller") {
            raw.relay.congestion_controller = cgstn_ctrl.parse()?;
        };

        if let Some(timeout) = matches.opt_str("max-idle-time") {
            raw.relay.max_idle_time = timeout.parse()?;
        };

        if let Some(interval) = matches.opt_str("heartbeat-interval") {
            raw.relay.heartbeat_interval = interval.parse()?;
        };

        let alpn = matches.opt_strs("alpn");

        if !alpn.is_empty() {
            raw.relay.alpn = alpn;
        }

        raw.relay.disable_sni |= matches.opt_present("disable-sni");
        raw.relay.ipv6_endpoint |= matches.opt_present("ipv6-endpoint");
        raw.relay.reduce_rtt |= matches.opt_present("reduce-rtt");

        if let Some(local_ip) = matches.opt_str("local-ip") {
            raw.local.ip = local_ip.parse()?;
        };

        raw.local.username = matches.opt_str("local-username").or(raw.local.username);
        raw.local.password = matches.opt_str("local-password").or(raw.local.password);

        if let Some(max_udp_packet_size) = matches.opt_str("max-udp-packet-size") {
            raw.max_udp_packet_size = max_udp_packet_size.parse()?;
        };

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
            Ok(Self::Cubic)
        } else if s.eq_ignore_ascii_case("new_reno") || s.eq_ignore_ascii_case("newreno") {
            Ok(Self::NewReno)
        } else if s.eq_ignore_ascii_case("bbr") {
            Ok(Self::Bbr)
        } else {
            Err(ConfigError::InvalidCongestionController)
        }
    }
}

impl FromStr for UdpMode {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("native") {
            Ok(Self::Native)
        } else if s.eq_ignore_ascii_case("quic") {
            Ok(Self::Quic)
        } else {
            Err(ConfigError::InvalidUdpRelayMode)
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

    pub(super) const fn udp_mode() -> UdpMode {
        UdpMode::Native
    }

    pub(super) const fn congestion_controller() -> CongestionController {
        CongestionController::Cubic
    }

    pub(super) const fn max_idle_time() -> u32 {
        15000
    }

    pub(super) const fn heartbeat_interval() -> u64 {
        10000
    }

    pub(super) const fn alpn() -> Vec<String> {
        Vec::new()
    }

    pub(super) const fn disable_sni() -> bool {
        false
    }

    pub(super) const fn ipv6_endpoint() -> bool {
        false
    }

    pub(super) const fn reduce_rtt() -> bool {
        false
    }

    pub(super) const fn local_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    pub(super) const fn max_udp_packet_size() -> usize {
        1536
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
    #[error("Invalid udp relay mode")]
    InvalidUdpRelayMode,
    #[error("Failed to load the certificate: {0}")]
    Certificate(#[from] WebpkiError),
    #[error("Could not load platform certs: {0}")]
    NativeCertificate(#[source] IoError),
    #[error("Username and password must be set together for the local socks5 server")]
    LocalAuthentication,
    #[error(transparent)]
    ParseLogLevel(#[from] ParseLevelError),
}
