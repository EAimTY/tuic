use crate::{
    certificate,
    relay::{ServerAddr, UdpMode},
    socks5::Authentication as Socks5Authentication,
};
use getopts::{Fail, Options};
use log::{LevelFilter, ParseLevelError};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ClientConfig, IdleTimeout, TransportConfig, VarInt,
};
use rustls::RootCertStore;
use std::{
    io::Error as IoError,
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    num::ParseIntError,
    sync::Arc,
};
use thiserror::Error;
use webpki::Error as WebpkiError;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();

        opts.optopt(
            "s",
            "server",
            "(Required) Set the server address. This address must be included in certificate",
            "SERVER",
        );

        opts.optopt(
            "p",
            "server-port",
            "(Required) Set the server port",
            "SERVER_PORT",
        );

        opts.optopt(
            "t",
            "token",
            "(Required) Set the token for TUIC authentication",
            "TOKEN",
        );

        opts.optopt(
            "l",
            "local-port",
            "(Required) Set the listening port for local socks5 server",
            "LOCAL_PORT",
        );

        opts.optopt(
            "",
            "server-ip",
            "Set the server IP, for overwriting the DNS lookup result of the server address set in option '-s'",
            "SERVER_IP",
        );

        opts.optopt(
            "",
            "socks5-username",
            "Set the username for local socks5 server authentication",
            "SOCKS5_USERNAME",
        );

        opts.optopt(
            "",
            "socks5-password",
            "Set the password for local socks5 server authentication",
            "SOCKS5_PASSWORD",
        );

        opts.optflag("", "local-ipv6", "Enable IPv6 for local socks5 server");

        opts.optflag(
            "",
            "allow-external-connection",
            "Allow external connections for local socks5 server",
        );

        opts.optopt(
            "",
            "cert",
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
            r#"Set the congestion controller. Available: "cubic", "new_reno", "bbr". Default: "cubic""#,
            "CONGESTION_CONTROLLER",
        );

        opts.optopt(
            "",
            "max-idle-time",
            "Set the maximum idle time for connections, in milliseconds. The true idle timeout is the minimum of this and the server's one. Default: 15000",
            "MAX_IDLE_TIME",
        );

        opts.optopt(
            "",
            "heartbeat-interval",
            "Set the heartbeat interval, in milliseconds. This ensures that the QUIC connection is not closed when there are relay tasks but no data transfer. Default: 10000",
            "HEARTBEAT_INTERVAL",
        );

        opts.optopt(
            "",
            "max-udp-packet-size",
            "Set the maximum UDP packet size, in bytes. Excess bytes may be discarded. Default: 1536",
            "MAX_UDP_PACKET_SIZE",
        );

        opts.optflag("", "reduce-rtt", "Enable 0-RTT QUIC handshake");

        opts.optopt(
            "",
            "log-level",
            r#"Set the log level. Available: "off", "error", "warn", "info", "debug", "trace". Default: "info""#,
            "LOG_LEVEL",
        );

        opts.optflag("v", "version", "Print the version");
        opts.optflag("h", "help", "Print this help menu");

        Self {
            opts,
            program: None,
        }
    }

    pub fn get_usage(&self) -> String {
        self.opts.usage(&format!(
            "Usage: {} [options]",
            self.program.unwrap_or(env!("CARGO_PKG_NAME"))
        ))
    }

    pub fn parse(&mut self, args: &'cfg [String]) -> Result<Config, ConfigError> {
        self.program = Some(&args[0]);
        let matches = self.opts.parse(&args[1..])?;

        if matches.opt_present("h") {
            return Err(ConfigError::Help(self.get_usage()));
        }

        if matches.opt_present("v") {
            return Err(ConfigError::Version(env!("CARGO_PKG_VERSION")));
        }

        if !matches.free.is_empty() {
            return Err(ConfigError::UnexpectedArgument(matches.free.join(", ")));
        }

        let heartbeat_interval = if let Some(interval) = matches.opt_str("heartbeat-interval") {
            interval.parse()?
        } else {
            10000
        };

        let config = {
            let mut config = if let Some(path) = matches.opt_str("cert") {
                let mut certs = RootCertStore::empty();

                for cert in certificate::load_certificates(&path)
                    .map_err(|err| ConfigError::Io(path, err))?
                {
                    certs.add(&cert)?;
                }

                ClientConfig::with_root_certificates(certs)
            } else {
                ClientConfig::with_native_roots()
            };

            let mut transport = TransportConfig::default();

            match matches.opt_str("congestion-controller") {
                None => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
                }
                Some(ctrl) if ctrl.eq_ignore_ascii_case("cubic") => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
                }
                Some(ctrl)
                    if ctrl.eq_ignore_ascii_case("newreno")
                        || ctrl.eq_ignore_ascii_case("new_reno") =>
                {
                    transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
                }
                Some(ctrl) if ctrl.eq_ignore_ascii_case("bbr") => {
                    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
                }
                Some(ctrl) => return Err(ConfigError::CongestionController(ctrl)),
            }

            let max_idle_time = if let Some(timeout) = matches.opt_str("max-idle-time") {
                timeout.parse()?
            } else {
                15000
            };

            if max_idle_time as u64 <= heartbeat_interval {
                return Err(ConfigError::HeartbeatInterval);
            }

            transport.max_idle_timeout(Some(IdleTimeout::from(VarInt::from_u32(max_idle_time))));

            config.transport = Arc::new(transport);
            config
        };

        let server_addr = {
            let server_name = match matches.opt_str("s") {
                Some(server) => server,
                None => return Err(ConfigError::RequiredOptionMissing("--server")),
            };

            let server_port = match matches.opt_str("p") {
                Some(port) => port.parse()?,
                None => return Err(ConfigError::RequiredOptionMissing("--port")),
            };

            if let Some(server_ip) = matches.opt_str("server-ip") {
                let server_ip = server_ip.parse()?;

                let server_addr = SocketAddr::new(server_ip, server_port);

                ServerAddr::SocketAddr {
                    server_addr,
                    server_name,
                }
            } else {
                ServerAddr::HostnameAddr {
                    hostname: server_name,
                    server_port,
                }
            }
        };

        let token_digest = match matches.opt_str("t") {
            Some(token) => *blake3::hash(&token.into_bytes()).as_bytes(),
            None => return Err(ConfigError::RequiredOptionMissing("--token")),
        };

        let local_addr = {
            let local_port = match matches.opt_str("l") {
                Some(port) => port.parse()?,
                None => return Err(ConfigError::RequiredOptionMissing("--local-port")),
            };

            let local_ip = match (
                matches.opt_present("local-ipv6"),
                matches.opt_present("allow-external-connection"),
            ) {
                (false, false) => IpAddr::V4(Ipv4Addr::LOCALHOST),
                (false, true) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                (true, false) => IpAddr::V6(Ipv6Addr::LOCALHOST),
                (true, true) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            };

            SocketAddr::from((local_ip, local_port))
        };

        let socks5_authentication = match (
            matches.opt_str("socks5-username"),
            matches.opt_str("socks5-password"),
        ) {
            (None, None) => Socks5Authentication::None,
            (Some(username), Some(password)) => Socks5Authentication::Password {
                username: username.into_bytes(),
                password: password.into_bytes(),
            },
            _ => return Err(ConfigError::Socks5Authentication),
        };

        let udp_mode = match matches.opt_str("udp-mode") {
            None => UdpMode::Native,
            Some(mode) if mode.eq_ignore_ascii_case("native") => UdpMode::Native,
            Some(mode) if mode.eq_ignore_ascii_case("quic") => UdpMode::Quic,
            Some(mode) => return Err(ConfigError::UdpMode(mode)),
        };

        let max_udp_packet_size = if let Some(size) = matches.opt_str("max-udp-packet-size") {
            size.parse()?
        } else {
            1536
        };

        let reduce_rtt = matches.opt_present("reduce-rtt");

        let log_level = if let Some(level) = matches.opt_str("log-level") {
            level.parse()?
        } else {
            LevelFilter::Info
        };

        Ok(Config {
            config,
            server_addr,
            token_digest,
            local_addr,
            socks5_authentication,
            udp_mode,
            heartbeat_interval,
            max_udp_packet_size,
            reduce_rtt,
            log_level,
        })
    }
}

pub struct Config {
    pub config: ClientConfig,
    pub server_addr: ServerAddr,
    pub token_digest: [u8; 32],
    pub local_addr: SocketAddr,
    pub socks5_authentication: Socks5Authentication,
    pub udp_mode: UdpMode,
    pub heartbeat_interval: u64,
    pub max_udp_packet_size: usize,
    pub reduce_rtt: bool,
    pub log_level: LevelFilter,
}

#[derive(Error, Debug)]
pub enum ConfigError<'e> {
    #[error("{0}")]
    Help(String),
    #[error("{0}")]
    Version(&'e str),
    #[error(transparent)]
    ParseArgument(#[from] Fail),
    #[error("Unexpected argument: {0}")]
    UnexpectedArgument(String),
    #[error("Required option '{0}' missing")]
    RequiredOptionMissing(&'e str),
    #[error("Failed to read '{0}': {1}")]
    Io(String, #[source] IoError),
    #[error("Failed to load certificate: {0}")]
    Certificate(#[from] WebpkiError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    ParseIpAddr(#[from] AddrParseError),
    #[error("Unknown congestion controller: {0}")]
    CongestionController(String),
    #[error("Heartbeat interval must be less than the max idle time")]
    HeartbeatInterval,
    #[error("Unknown udp mode: {0}")]
    UdpMode(String),
    #[error("Socks5 username and password must be set together")]
    Socks5Authentication,
    #[error(transparent)]
    ParseLogLevel(#[from] ParseLevelError),
}
