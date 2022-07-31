use super::{
    stream::{RecvStream, SendStream, StreamReg},
    ConnectError, Stream,
};
use crate::{
    protocol::{Address, Command, Error as TuicError},
    UdpRelayMode,
};
use quinn::{
    Connecting as QuinnConnecting, Connection as QuinnConnection, Datagrams, IncomingUniStreams,
    NewConnection as QuinnNewConnection,
};
use std::{
    io::{Error as IoError, Result as IoResult},
    sync::Arc,
};

#[derive(Debug)]
pub struct Connecting {
    conn: QuinnConnecting,
    token: [u8; 32],
    udp_relay_mode: UdpRelayMode,
}

impl Connecting {
    pub(super) fn new(
        conn: QuinnConnecting,
        token: [u8; 32],
        udp_relay_mode: UdpRelayMode,
    ) -> Self {
        Self {
            conn,
            token,
            udp_relay_mode,
        }
    }

    pub async fn establish(self) -> Result<(Connection, IncomingPackets), ConnectError> {
        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = match self.conn.await {
            Ok(conn) => conn,
            Err(err) => return Err(ConnectError::from_quinn_connection_error(err)),
        };

        Ok(Connection::new(
            connection,
            uni_streams,
            datagrams,
            self.token,
            self.udp_relay_mode,
        ))
    }
}

#[derive(Debug)]
pub struct Connection {
    conn: QuinnConnection,
    token: [u8; 32],
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
}

impl Connection {
    pub(super) fn new(
        conn: QuinnConnection,
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        token: [u8; 32],
        udp_relay_mode: UdpRelayMode,
    ) -> (Self, IncomingPackets) {
        let stream_reg = Arc::new(Arc::new(()));

        let conn = Self {
            conn,
            token,
            udp_relay_mode,
            stream_reg: stream_reg.clone(),
        };

        let incoming = IncomingPackets {
            uni_streams,
            datagrams,
            udp_relay_mode,
            stream_reg,
        };

        (conn, incoming)
    }

    pub async fn authenticate(&self) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Authenticate(self.token);
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn heartbeat(&self) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Heartbeat;
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn connect(&self, addr: Address) -> IoResult<Option<Stream>> {
        let mut stream = self.get_bi_stream().await?;

        let cmd = Command::Connect { addr };
        cmd.write_to(&mut stream).await?;

        let resp = match Command::read_from(&mut stream).await {
            Ok(Command::Response(resp)) => Ok(resp),
            Ok(cmd) => Err(TuicError::InvalidCommand(cmd.type_code())),
            Err(err) => Err(err),
        };

        let res = match resp {
            Ok(true) => return Ok(Some(stream)),
            Ok(false) => Ok(None),
            Err(err) => Err(IoError::from(err)),
        };

        stream.finish().await?;
        res
    }

    pub async fn packet(&self) -> IoResult<()> {
        todo!()
    }

    pub async fn dissociate(&self) -> IoResult<()> {
        todo!()
    }

    async fn get_send_stream(&self) -> IoResult<SendStream> {
        let send = self.conn.open_uni().await?;
        Ok(SendStream::new(send, self.stream_reg.as_ref().clone()))
    }

    async fn get_bi_stream(&self) -> IoResult<Stream> {
        let (send, recv) = self.conn.open_bi().await?;
        let send = SendStream::new(send, self.stream_reg.as_ref().clone());
        let recv = RecvStream::new(recv, self.stream_reg.as_ref().clone());
        Ok(Stream::new(send, recv))
    }
}

#[derive(Debug)]
pub struct IncomingPackets {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
}
