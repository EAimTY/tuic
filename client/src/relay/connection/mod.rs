use super::Address;
use crate::config::UdpMode;
use anyhow::Result;
use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError as QuinnConnectionError, Datagrams,
    IncomingUniStreams, NewConnection, SendDatagramError, WriteError,
};
use std::{
    collections::HashMap,
    io::Error as IoError,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::mpsc::Sender as MpscSender;
use tuic_protocol::{Command as TuicCommand, Error as TuicError};

mod dispatch;
mod task;

#[derive(Clone)]
pub struct Connection {
    controller: QuinnConnection,
    udp_mode: UdpMode,
    udp_sessions: Arc<UdpSessionMap>,
    is_closed: Arc<AtomicBool>,
}

pub type UdpSessionMap = Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>;

impl Connection {
    pub async fn init(
        conn: Connecting,
        token_digest: [u8; 32],
        udp_mode: UdpMode,
        reduce_rtt: bool,
    ) -> Result<Self> {
        let NewConnection {
            connection,
            uni_streams,
            datagrams,
            ..
        } = if reduce_rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => conn.await?,
            }
        } else {
            conn.await?
        };

        let udp_sessions = Arc::new(Mutex::new(HashMap::new()));
        let is_closed = Arc::new(AtomicBool::new(false));

        let conn = Self {
            controller: connection,
            udp_mode,
            udp_sessions: udp_sessions.clone(),
            is_closed: is_closed.clone(),
        };

        tokio::spawn(Self::authenticate(conn.clone(), token_digest));

        match udp_mode {
            UdpMode::Native => tokio::spawn(Self::listen_uni_streams(conn.clone(), uni_streams)),
            UdpMode::Quic => tokio::spawn(Self::listen_datagrams(conn.clone(), datagrams)),
        };

        Ok(conn)
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::Acquire)
    }

    async fn authenticate(self, token_digest: [u8; 32]) {
        async fn send_authenticate(
            conn: QuinnConnection,
            token_digest: [u8; 32],
        ) -> Result<(), Error> {
            let mut stream = conn.open_uni().await?;
            let cmd = TuicCommand::new_authenticate(token_digest);
            cmd.write_to(&mut stream).await?;
            Ok(())
        }

        match send_authenticate(self.controller, token_digest).await {
            Ok(()) => (),
            Err(err) => {
                self.is_closed.store(true, Ordering::Release);
                eprintln!("{err}");
            }
        }
    }

    async fn listen_uni_streams(self, uni_streams: IncomingUniStreams) {
        async fn listen(
            conn: Connection,
            mut uni_streams: IncomingUniStreams,
        ) -> Result<(), Error> {
            while let Some(stream) = uni_streams.next().await {
                let stream = stream?;
                let conn = conn.clone();

                tokio::spawn(async move {
                    match conn.process_incoming_uni_stream(stream).await {
                        Ok(()) => (),
                        Err(err) => {
                            eprintln!("{err}");
                        }
                    }
                });
            }

            Err(QuinnConnectionError::LocallyClosed)?
        }

        let is_closed = self.is_closed.clone();

        match listen(self, uni_streams).await {
            Ok(())
            | Err(Error::Quinn(QuinnConnectionError::LocallyClosed))
            | Err(Error::Quinn(QuinnConnectionError::TimedOut)) => (),
            Err(err) => eprintln!("{err}"),
        }

        is_closed.store(true, Ordering::Release);
    }

    async fn listen_datagrams(self, datagrams: Datagrams) {
        async fn listen(conn: Connection, mut datagrams: Datagrams) -> Result<(), Error> {
            while let Some(datagram) = datagrams.next().await {
                let datagram = datagram?;
                let conn = conn.clone();

                tokio::spawn(async move {
                    match conn.process_incoming_datagram(datagram).await {
                        Ok(()) => (),
                        Err(err) => {
                            eprintln!("{err}");
                        }
                    }
                });
            }

            Err(QuinnConnectionError::LocallyClosed)?
        }

        let is_closed = self.is_closed.clone();

        match listen(self, datagrams).await {
            Ok(())
            | Err(Error::Quinn(QuinnConnectionError::LocallyClosed))
            | Err(Error::Quinn(QuinnConnectionError::TimedOut)) => (),
            Err(err) => eprintln!("{err}"),
        }

        is_closed.store(true, Ordering::Release);
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Protocol(#[from] TuicError),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Quinn(#[from] QuinnConnectionError),
    #[error(transparent)]
    WriteStream(#[from] WriteError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
    #[error("UDP session not found: {0}")]
    UdpSessionNotFound(u32),
    #[error("bad command")]
    BadCommand,
}
