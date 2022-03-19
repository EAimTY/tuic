use crate::{
    certificate::{self, CertificateError},
    server::{CongestionController, ParseCongestionControllerError},
};
use getopts::{Fail, Options};
use log::{LevelFilter, ParseLevelError};
use rustls::{Certificate, PrivateKey};
use std::{num::ParseIntError, time::Duration};
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
            "Set the listening port(Required)",
            "SERVER_PORT",
        );

        opts.optopt(
            "t",
            "token",
            "Set the TUIC token for the authentication(Required)",
            "TOKEN",
        );

        opts.optopt(
            "c",
            "cert",
            "Set the certificate for QUIC handshake(Required)",
            "CERTIFICATE",
        );

        opts.optopt(
            "k",
            "priv-key",
            "Set the private key for QUIC handshake(Required)",
            "PRIVATE_KEY",
        );

        opts.optopt(
            "",
            "authentication-timeout",
            "Set the maximum time allowed between connection establishment and authentication packet receipt, in milliseconds. Default: 1000ms",
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

        let port = match matches.opt_str("p") {
            Some(port) => port.parse()?,
            None => return Err(ConfigError::RequiredOptionMissing("port")),
        };

        let token_digest = match matches.opt_str("t") {
            Some(token) => *blake3::hash(&token.into_bytes()).as_bytes(),
            None => return Err(ConfigError::RequiredOptionMissing("token")),
        };

        let certificates = match matches.opt_str("c") {
            Some(path) => certificate::load_certificates(&path)?,
            None => return Err(ConfigError::RequiredOptionMissing("cert")),
        };

        let private_key = match matches.opt_str("k") {
            Some(path) => certificate::load_private_key(&path)?,
            None => return Err(ConfigError::RequiredOptionMissing("priv-key")),
        };

        let authentication_timeout =
            if let Some(duration) = matches.opt_str("authentication-timeout") {
                let duration = duration.parse()?;
                Duration::from_millis(duration)
            } else {
                Duration::from_millis(1000)
            };

        let congestion_controller =
            if let Some(controller) = matches.opt_str("congestion-controller") {
                controller.parse()?
            } else {
                CongestionController::Cubic
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
            port,
            token_digest,
            certificates,
            private_key,
            authentication_timeout,
            congestion_controller,
            max_udp_packet_size,
            log_level,
        })
    }
}

pub struct Config {
    pub port: u16,
    pub token_digest: [u8; 32],
    pub certificates: Vec<Certificate>,
    pub private_key: PrivateKey,
    pub authentication_timeout: Duration,
    pub congestion_controller: CongestionController,
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
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error("Failed to load certificate / private key: {0}")]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    ParseCongestionController(#[from] ParseCongestionControllerError),
    #[error(transparent)]
    ParseLogLevel(#[from] ParseLevelError),
}
