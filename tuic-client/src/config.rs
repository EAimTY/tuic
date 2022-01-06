use getopts::{Fail, Options};
use thiserror::Error;

pub struct ConfigBuilder<'cfg> {
    opts: Options,
    program: Option<&'cfg str>,
}

impl<'cfg> ConfigBuilder<'cfg> {
    pub fn new() -> Self {
        let mut opts = Options::new();
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

        let matches = self.opts.parse(&args[1..])?;

        if !matches.free.is_empty() {
            return Err(ConfigError::UnexpectedArgument(matches.free.join(", ")));
        }

        if matches.opt_present("h") {
            return Err(ConfigError::Help);
        }

        let config = Config;

        Ok(config)
    }
}

pub struct Config;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    FailedToParseConfig(#[from] Fail),
    #[error("Unexpected urgument: `{0}`")]
    UnexpectedArgument(String),
    #[error("")]
    Help,
}
