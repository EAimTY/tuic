use getopts::{Fail, Options};
use std::{
    net::{AddrParseError, SocketAddr},
    num::ParseIntError,
};
use thiserror::Error;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();

        opts.reqopt(
            "s",
            "server",
            "Set the server address. This address is supposed to be in the certificate(Required)",
            "SERVER",
        );
        opts.reqopt(
            "p",
            "server-port",
            "Set the server port(Required)",
            "SERVER_PORT",
        );
        opts.reqopt(
            "t",
            "token",
            "Set the TUIC token for the server authentication(Required)",
            "TOKEN",
        );
        opts.reqopt(
            "l",
            "local-port",
            "Set the listening port of the local socks5 server(Required)",
            "LOCAL_PORT",
        );

        opts.optopt(
            "",
            "username",
            "Set the username of the local socks5 server",
            "USERNAME",
        );
        opts.optopt(
            "",
            "password",
            "Set the password of the local socks5 server",
            "PASSWORD",
        );

        opts.optopt(
            "",
            "server-ip",
            "Set the server IP, for overwriting the DNS lookup result of the server address",
            "SERVER_IP",
        );

        opts.optopt(
            "",
            "number-of-retries",
            "Set the number of retries for TUIC connection establishment (default: 3)",
            "NUMBER_OF_RETRIES",
        );

        opts.optopt(
            "",
            "cert",
            "Set the custom certificate for QUIC handshake. If not set, the platform's native roots will be trusted",
            "CERTIFICATE",
        );

        opts.optflag(
            "",
            "allow-external-connection",
            "Allow external connections to the local socks5 server",
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
            self.program.unwrap_or("tuic-client")
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

        let server_addr = {
            let server_name = matches.opt_str("s").unwrap();

            let server_port = matches
                .opt_str("p")
                .unwrap()
                .parse()
                .map_err(|err| ConfigError::ParsePort(err, self.get_usage()))?;

            if let Some(server_ip) = matches.opt_str("server-ip") {
                let server_ip = server_ip
                    .parse()
                    .map_err(|err| ConfigError::ParseServerIp(err, self.get_usage()))?;

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

        let token = {
            let token = matches.opt_str("t").unwrap();
            seahash::hash(&token.into_bytes())
        };

        let number_of_retries =
            if let Some(number_of_retries) = matches.opt_str("number-of-retries") {
                number_of_retries
                    .parse()
                    .map_err(|err| ConfigError::ParseNumberOfRetries(err, self.get_usage()))?
            } else {
                3
            };

        let local_addr = {
            let local_port = matches
                .opt_str("l")
                .unwrap()
                .parse()
                .map_err(|err| ConfigError::ParsePort(err, self.get_usage()))?;

            if matches.opt_present("allow-external-connection") {
                SocketAddr::from(([0, 0, 0, 0], local_port))
            } else {
                SocketAddr::from(([127, 0, 0, 1], local_port))
            }
        };

        let certificate_path = matches.opt_str("cert");

        let username = matches.opt_str("username");

        let password = matches.opt_str("password");

        Ok(Config {
            server_addr,
            token,
            number_of_retries,
            local_addr,
            certificate_path,
            username,
            password,
        })
    }
}

pub struct Config {
    pub server_addr: ServerAddr,
    pub token: u64,
    pub number_of_retries: usize,
    pub local_addr: SocketAddr,
    pub certificate_path: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone)]
pub enum ServerAddr {
    SocketAddr {
        server_addr: SocketAddr,
        server_name: String,
    },
    HostnameAddr {
        hostname: String,
        server_port: u16,
    },
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}\n\n{1}")]
    Parse(Fail, String),
    #[error("Unexpected urgument: {0}\n\n{1}")]
    UnexpectedArgument(String, String),
    #[error("Failed to parse the port: {0}\n\n{1}")]
    ParsePort(ParseIntError, String),
    #[error("Failed to parse the server IP: {0}\n\n{1}")]
    ParseServerIp(AddrParseError, String),
    #[error("Failed to parse the number of retries: {0}\n\n{1}")]
    ParseNumberOfRetries(ParseIntError, String),
    #[error("{0}")]
    Version(&'static str),
    #[error("{0}")]
    Help(String),
}
