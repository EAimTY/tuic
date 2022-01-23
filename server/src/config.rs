use getopts::{Fail, Options};
use log::Level as LogLevel;
use std::num::ParseIntError;
use thiserror::Error;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();

        opts.reqopt(
            "p",
            "port",
            "Set the listening port(Required)",
            "SERVER_PORT",
        );
        opts.reqopt(
            "t",
            "token",
            "Set the TUIC token for the authentication(Required)",
            "TOKEN",
        );

        opts.reqopt(
            "c",
            "cert",
            "Set the certificate for QUIC handshake(Required)",
            "CERTIFICATE",
        );
        opts.reqopt(
            "k",
            "priv-key",
            "Set the private key for QUIC handshake(Required)",
            "PRIVATE_KEY",
        );

        opts.optflag(
            "",
            "log-level",
            "Set the log level. 0 - off, 1 - error, 2 - warn (default), 3 - info, 4 - debug",
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
            self.program.unwrap_or("tuic-server")
        ))
    }

    pub fn parse(&mut self, args: &'cfg [String]) -> Result<Config, ConfigError> {
        self.program = Some(&args[0]);

        let matches = self
            .opts
            .parse(&args[1..])
            .map_err(|err| ConfigError::Parse(err, self.get_usage()))?;

        if !matches.free.is_empty() {
            return Err(ConfigError::UnexpectedArgument(
                matches.free.join(", "),
                self.get_usage(),
            ));
        }

        if matches.opt_present("v") {
            return Err(ConfigError::Version(env!("CARGO_PKG_VERSION")));
        }

        if matches.opt_present("h") {
            return Err(ConfigError::Help(self.get_usage()));
        }

        let port = unsafe { matches.opt_str("p").unwrap_unchecked() }
            .parse()
            .map_err(|err| ConfigError::ParsePort(err, self.get_usage()))?;

        let token = {
            let token = unsafe { matches.opt_str("t").unwrap_unchecked() };
            seahash::hash(&token.into_bytes())
        };

        let certificate_path = unsafe { matches.opt_str("c").unwrap_unchecked() };
        let private_key_path = unsafe { matches.opt_str("k").unwrap_unchecked() };

        let log_level = if let Some(level) = matches.opt_str("log-level") {
            match level.parse() {
                Ok(0) => None,
                Ok(1) => Some(LogLevel::Error),
                Ok(2) => Some(LogLevel::Warn),
                Ok(3) => Some(LogLevel::Info),
                Ok(4) => Some(LogLevel::Debug),
                _ => return Err(ConfigError::ParseLogLevel(level, self.get_usage())),
            }
        } else {
            Some(LogLevel::Warn)
        };

        Ok(Config {
            token,
            port,
            certificate_path,
            private_key_path,
            log_level,
        })
    }
}

pub struct Config {
    pub port: u16,
    pub token: u64,
    pub certificate_path: String,
    pub private_key_path: String,
    pub log_level: Option<LogLevel>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}\n\n{1}")]
    Parse(Fail, String),
    #[error("Unexpected urgument: {0}\n\n{1}")]
    UnexpectedArgument(String, String),
    #[error("Failed to parse the port: {0}\n\n{1}")]
    ParsePort(ParseIntError, String),
    #[error("Failed to parse the log level: {0}\n\n{1}")]
    ParseLogLevel(String, String),
    #[error("{0}")]
    Version(&'static str),
    #[error("{0}")]
    Help(String),
}
