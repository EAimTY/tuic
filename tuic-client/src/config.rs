use lexopt::{Arg, Error as ArgumentError, Parser};
use serde::Deserialize;
use serde_json::Error as SerdeError;
use std::{ffi::OsString, fs::File, io::Error as IoError};
use thiserror::Error;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {}

impl Config {
    pub fn parse<A>(args: A) -> Result<Self, ConfigError>
    where
        A: IntoIterator,
        A::Item: Into<OsString>,
    {
        let mut parser = Parser::from_iter(args);
        let mut path = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Short('c') | Arg::Long("config") => {
                    if path.is_none() {
                        path = Some(parser.value()?);
                    } else {
                        return Err(ConfigError::Argument(arg.unexpected()));
                    }
                }
                Arg::Short('v') | Arg::Long("version") => {
                    return Err(ConfigError::Version(env!("CARGO_PKG_VERSION")))
                }
                Arg::Short('h') | Arg::Long("help") => return Err(ConfigError::Help(todo!())),
                _ => return Err(ConfigError::Argument(arg.unexpected())),
            }
        }

        if path.is_none() {
            return Err(ConfigError::NoConfig);
        }

        let file = File::open(path.unwrap())?;

        Ok(serde_json::from_reader(file)?)
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("transparent")]
    Argument(#[from] ArgumentError),
    #[error("no config file specified")]
    NoConfig,
    #[error("{0}")]
    Version(&'static str),
    #[error("{0}")]
    Help(&'static str),
    #[error("transparent")]
    Io(#[from] IoError),
    #[error("transparent")]
    Serde(#[from] SerdeError),
}
