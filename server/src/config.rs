use crate::certificate;
use getopts::{Fail, Options};
use log::{LevelFilter, ParseLevelError};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ServerConfig, TransportConfig,
};
use rustls::Error as RustlsError;
use std::{io::Error as IoError, num::ParseIntError, sync::Arc, time::Duration};
use thiserror::Error;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();

        opts.optopt(
            "p",
            "port",
            "(Required) Set the listening port",
            "SERVER_PORT",
        );

        opts.optopt(
            "t",
            "token",
            "(Required) Set the token for TUIC authentication",
            "TOKEN",
        );

        opts.optopt(
            "c",
            "cert",
            "(Required) Set the X.509 certificate. This must be an end-entity certificate",
            "CERTIFICATE",
        );

        opts.optopt(
            "k",
            "priv-key",
            "(Required) Set the private key. Supports PKCS#8 and PKCS#1(RSA) formats",
            "PRIVATE_KEY",
        );

        opts.optopt(
            "",
            "authentication-timeout",
            "Set the maximum time allowed between a QUIC connection established and the TUIC authentication packet received, in milliseconds. Default: 1000ms",
            "AUTHENTICATION_TIMEOUT",
        );

        opts.optopt(
            "",
            "congestion-controller",
            r#"Set the congestion controller. Available: "cubic", "new_reno", "bbr". Default: "cubic""#,
            "CONGESTION_CONTROLLER",
        );

        opts.optopt(
            "",
            "max-udp-packet-size",
            "Set the maximum UDP packet size. Excess bytes may be discarded. Default: 1536",
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

        let config = {
            let certs = match matches.opt_str("c") {
                Some(path) => certificate::load_certificates(&path)
                    .map_err(|err| ConfigError::Io(path, err))?,
                None => return Err(ConfigError::RequiredOptionMissing("cert")),
            };

            let priv_key = match matches.opt_str("k") {
                Some(path) => certificate::load_private_key(&path)
                    .map_err(|err| ConfigError::Io(path, err))?,
                None => return Err(ConfigError::RequiredOptionMissing("priv-key")),
            };

            let mut config = ServerConfig::with_single_cert(certs, priv_key)?;
            let mut transport = TransportConfig::default();

            match matches.opt_str("congestion-controller") {
                None => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
                }
                Some(ctrl) if ctrl.eq_ignore_ascii_case("cubic") => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
                }
                Some(ctrl) if ctrl.eq_ignore_ascii_case("new_reno") => {
                    transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
                }
                Some(ctrl) if ctrl.eq_ignore_ascii_case("bbr") => {
                    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
                }
                Some(ctrl) => return Err(ConfigError::CongestionController(ctrl)),
            }

            config.transport = Arc::new(transport);
            config
        };

        let port = match matches.opt_str("p") {
            Some(port) => port.parse()?,
            None => return Err(ConfigError::RequiredOptionMissing("port")),
        };

        let token_digest = match matches.opt_str("t") {
            Some(token) => *blake3::hash(&token.into_bytes()).as_bytes(),
            None => return Err(ConfigError::RequiredOptionMissing("token")),
        };

        let authentication_timeout =
            if let Some(duration) = matches.opt_str("authentication-timeout") {
                let duration = duration.parse()?;
                Duration::from_millis(duration)
            } else {
                Duration::from_millis(1000)
            };

        let max_udp_packet_size = if let Some(size) = matches.opt_str("max-udp-packet-size") {
            size.parse()?
        } else {
            1536
        };

        let log_level = if let Some(level) = matches.opt_str("log-level") {
            level.parse()?
        } else {
            LevelFilter::Info
        };

        Ok(Config {
            config,
            port,
            token_digest,
            authentication_timeout,
            max_udp_packet_size,
            log_level,
        })
    }
}

pub struct Config {
    pub config: ServerConfig,
    pub port: u16,
    pub token_digest: [u8; 32],
    pub authentication_timeout: Duration,
    pub max_udp_packet_size: usize,
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
    #[error("Failed to read {0}: {1}")]
    Io(String, #[source] IoError),
    #[error("Failed to load certificate / private key: {0}")]
    Rustls(#[from] RustlsError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error("Unknown congestion controller: {0}")]
    CongestionController(String),
    #[error(transparent)]
    ParseLogLevel(#[from] ParseLevelError),
}
