use super::{Address, RelayError};
use crate::config::UdpMode;
use anyhow::Result;
use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError, Datagrams, IncomingUniStreams,
    NewConnection,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc::Sender as MpscSender;
use tuic_protocol::Command as TuicCommand;

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
            UdpMode::Native => tokio::spawn(Self::listen_datagrams(conn.clone(), datagrams)),
            UdpMode::Quic => tokio::spawn(Self::listen_uni_streams(conn.clone(), uni_streams)),
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
        ) -> Result<(), RelayError> {
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
        ) -> Result<(), RelayError> {
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

            Err(ConnectionError::LocallyClosed)?
        }

        let is_closed = self.is_closed.clone();

        match listen(self, uni_streams).await {
            Ok(())
            | Err(RelayError::Connection(ConnectionError::LocallyClosed))
            | Err(RelayError::Connection(ConnectionError::TimedOut)) => {}
            Err(err) => eprintln!("{err}"),
        }

        is_closed.store(true, Ordering::Release);
    }

    async fn listen_datagrams(self, datagrams: Datagrams) {
        async fn listen(conn: Connection, mut datagrams: Datagrams) -> Result<(), RelayError> {
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

            Err(ConnectionError::LocallyClosed)?
        }

        let is_closed = self.is_closed.clone();

        match listen(self, datagrams).await {
            Ok(())
            | Err(RelayError::Connection(ConnectionError::LocallyClosed))
            | Err(RelayError::Connection(ConnectionError::TimedOut)) => {}
            Err(err) => eprintln!("{err}"),
        }

        is_closed.store(true, Ordering::Release);
    }
}
