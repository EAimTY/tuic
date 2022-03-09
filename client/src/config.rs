use crate::{cert, socks5::Authentication as Socks5Auth};
use anyhow::{bail, Context, Result};
use getopts::Options;
use rustls::Certificate;
use std::net::SocketAddr;

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
            "Set the server address. This address is supposed to be in the certificate(Required)",
            "SERVER",
        );

        opts.optopt(
            "p",
            "server-port",
            "Set the server port(Required)",
            "SERVER_PORT",
        );

        opts.optopt(
            "t",
            "token",
            "Set the TUIC token for the server authentication(Required)",
            "TOKEN",
        );

        opts.optopt(
            "l",
            "local-port",
            "Set the listening port of the local socks5 server(Required)",
            "LOCAL_PORT",
        );

        opts.optopt(
            "",
            "server-ip",
            "Set the server IP, for overwriting the DNS lookup result of the server address",
            "SERVER_IP",
        );

        opts.optopt(
            "",
            "socks5-username",
            "Set the username of the local socks5 server authentication",
            "SOCKS5_USERNAME",
        );

        opts.optopt(
            "",
            "socks5-password",
            "Set the password of the local socks5 server authentication",
            "SOCKS5_PASSWORD",
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

        opts.optopt(
            "",
            "congestion-controller",
            r#"Set the congestion controller. Available: "cubic" (default), "new_reno", "bbr""#,
            "CONGESTION_CONTROLLER",
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

    pub fn parse(&mut self, args: &'cfg [String]) -> Result<Config> {
        self.program = Some(&args[0]);

        let matches = self.opts.parse(&args[1..])?;

        if matches.opt_present("h") {
            bail!("{}", self.get_usage());
        }

        if matches.opt_present("v") {
            bail!("{}", env!("CARGO_PKG_VERSION"));
        }

        if !matches.free.is_empty() {
            bail!("Unexpected argument: {}", matches.free.join(", "),);
        }

        let server_addr = {
            let server_name = matches
                .opt_str("s")
                .context("Required option 'server' missing")?;

            let server_port = matches
                .opt_str("p")
                .context("Required option 'port' missing")?
                .parse()?;

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

        let token_digest = {
            let token = matches
                .opt_str("t")
                .context("Required option 'token' missing")?;
            *blake3::hash(&token.into_bytes()).as_bytes()
        };

        let local_addr = {
            let local_port = matches
                .opt_str("l")
                .context("Required option 'local-port' missing")?
                .parse()?;

            if matches.opt_present("allow-external-connection") {
                SocketAddr::from(([0, 0, 0, 0], local_port))
            } else {
                SocketAddr::from(([127, 0, 0, 1], local_port))
            }
        };

        let socks5_auth = match (
            matches.opt_str("socks5-username"),
            matches.opt_str("socks5-password"),
        ) {
            (None, None) => Socks5Auth::None,
            (Some(username), Some(password)) => Socks5Auth::Password {
                username: username.into_bytes(),
                password: password.into_bytes(),
            },
            _ => bail!(
                "socks5 server username and password should be set together\n\n{}",
                self.get_usage()
            ),
        };

        let certificate = if let Some(path) = matches.opt_str("cert") {
            Some(cert::load_cert(&path)?)
        } else {
            None
        };

        let congestion_controller =
            if let Some(controller) = matches.opt_str("congestion-controller") {
                match controller.as_str() {
                    "cubic" => CongestionController::Cubic,
                    "new_reno" => CongestionController::NewReno,
                    "bbr" => CongestionController::Bbr,
                    _ => bail!("Unknown congestion controller: {}", controller),
                }
            } else {
                CongestionController::Cubic
            };

        Ok(Config {
            server_addr,
            token_digest,
            local_addr,
            socks5_auth,
            certificate,
            congestion_controller,
        })
    }
}

pub struct Config {
    pub server_addr: ServerAddr,
    pub token_digest: [u8; 32],
    pub local_addr: SocketAddr,
    pub socks5_auth: Socks5Auth,
    pub certificate: Option<Certificate>,
    pub congestion_controller: CongestionController,
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

pub enum CongestionController {
    Cubic,
    NewReno,
    Bbr,
}
