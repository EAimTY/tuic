use super::IncomingTasks;
use crate::UdpRelayMode;
use crossbeam_utils::atomic::AtomicCell;
use quinn::{
    Connecting as QuinnConnecting, Connection as QuinnConnection,
    ConnectionError as QuinnConnectionError, NewConnection as QuinnNewConnection,
};
use std::{io::Error as IoError, sync::Arc};
use thiserror::Error;

pub struct Connecting {
    conn: QuinnConnecting,
}

impl Connecting {
    pub(super) fn new(conn: QuinnConnecting) -> Self {
        Self { conn }
    }

    pub async fn establish(self) -> Result<(Connection, IncomingTasks), ConnectionError> {
        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = self
            .conn
            .await
            .map_err(ConnectionError::from_quinn_connection_error)?;

        let udp_relay_mode = Arc::new(AtomicCell::new(None));

        let conn = Connection::new(connection, udp_relay_mode.clone());
        let incoming = IncomingTasks::new(uni_streams, datagrams, udp_relay_mode);

        Ok((conn, incoming))
    }
}

pub struct Connection {
    conn: QuinnConnection,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
}

impl Connection {
    fn new(conn: QuinnConnection, udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>) -> Self {
        Self {
            conn,
            udp_relay_mode,
        }
    }
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error(transparent)]
    Io(#[from] IoError),
}

impl ConnectionError {
    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }
}
