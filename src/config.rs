use anyhow::{bail, Result};
use getopts::Options;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();
        opts.optflag("c", "client", "Run tuicsocks in the client mode");
        opts.optflag("s", "server", "Run tuicsocks in the server mode");
        opts.optflag("h", "help", "Print this help menu");

        Self {
            opts,
            program: None,
        }
    }

    pub fn get_usage(&self) -> String {
        self.opts.usage(&format!(
            "Usage: {} [options]",
            self.program.unwrap_or("tuicsocks")
        ))
    }

    pub fn parse(&mut self, args: &'cfg [String]) -> Result<Config> {
        self.program = Some(&args[0]);

        let matches = self.opts.parse(&args[1..])?;

        if !matches.free.is_empty() {
            bail!("unexpected arguments: {}", matches.free.join(", "));
        }

        if matches.opt_present("h") {
            bail!("");
        }

        let config = match (matches.opt_present("c"), matches.opt_present("s")) {
            (false, true) => Config::Client(ClientConfig),
            (true, false) => Config::Server(ServerConfig),
            _ => bail!("Running mode unspecified"),
        };

        Ok(config)
    }
}

pub enum Config {
    Client(ClientConfig),
    Server(ServerConfig),
}

pub struct ClientConfig;
pub struct ServerConfig;
