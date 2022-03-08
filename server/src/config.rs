use crate::cert;
use anyhow::{bail, Result};
use getopts::Options;
use rustls::{Certificate, PrivateKey};

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

        if !matches.free.is_empty() {
            bail!("Unexpected argument: {}", matches.free.join(", "),);
        }

        if matches.opt_present("v") {
            bail!("{}", env!("CARGO_PKG_VERSION"));
        }

        if matches.opt_present("h") {
            bail!("{}", self.get_usage());
        }

        let port = unsafe { matches.opt_str("p").unwrap_unchecked() }.parse()?;

        let token_digest = {
            let token = unsafe { matches.opt_str("t").unwrap_unchecked() };
            *blake3::hash(&token.into_bytes()).as_bytes()
        };

        let certificate = {
            let path = unsafe { matches.opt_str("c").unwrap_unchecked() };
            cert::load_cert(&path)?
        };

        let private_key = {
            let path = unsafe { matches.opt_str("k").unwrap_unchecked() };
            cert::load_priv_key(&path)?
        };

        Ok(Config {
            port,
            token_digest,
            certificate,
            private_key,
        })
    }
}

pub struct Config {
    pub port: u16,
    pub token_digest: [u8; 32],
    pub certificate: Certificate,
    pub private_key: PrivateKey,
}
