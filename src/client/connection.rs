use super::{
    stream::{RecvStream, SendStream, StreamReg},
    IncomingPackets, Stream,
};
use crate::{
    common,
    protocol::{Address, Command, Error as TuicError},
    UdpRelayMode,
};
use bytes::{Bytes, BytesMut};
use quinn::{
    Connecting as QuinnConnecting, Connection as QuinnConnection,
    ConnectionError as QuinnConnectionError, Datagrams, IncomingUniStreams,
    NewConnection as QuinnNewConnection, SendDatagramError as QuinnSendDatagramError,
};
use std::{
    io::{Error as IoError, ErrorKind},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

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

    pub async fn establish(self) -> Result<(Connection, IncomingPackets), ConnectionError> {
        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = self
            .conn
            .await
            .map_err(ConnectionError::from_quinn_connection_error)?;

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
    next_pkt_id: Arc<AtomicU16>,
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
            next_pkt_id: Arc::new(AtomicU16::new(0)),
        };

        let incoming = IncomingPackets::new(uni_streams, datagrams, udp_relay_mode, stream_reg);

        (conn, incoming)
    }

    pub async fn authenticate(&self) -> Result<(), ConnectionError> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Authenticate(self.token);
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn heartbeat(&self) -> Result<(), ConnectionError> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Heartbeat;
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn connect(&self, addr: Address) -> Result<Option<Stream>, ConnectionError> {
        let mut stream = self.get_bi_stream().await?;

        let cmd = Command::Connect { addr };
        cmd.write_to(&mut stream).await?;

        let resp = match Command::read_from(&mut stream).await {
            Ok(Command::Response(resp)) => Ok(resp),
            Ok(cmd) => Err(TuicError::InvalidCommand(cmd.as_type_code())),
            Err(err) => Err(err),
        };

        let res = match resp {
            Ok(true) => return Ok(Some(stream)),
            Ok(false) => Ok(None),
            Err(err) => Err(ConnectionError::from(err)),
        };

        stream.finish().await?;
        res
    }

    pub async fn packet(
        &self,
        assoc_id: u32,
        addr: Address,
        pkt: Bytes,
    ) -> Result<(), ConnectionError> {
        match self.udp_relay_mode {
            UdpRelayMode::Native => self.send_packet_to_datagram(assoc_id, addr, pkt),
            UdpRelayMode::Quic => self.send_packet_to_uni_stream(assoc_id, addr, pkt).await,
        }
    }

    pub async fn dissociate(&self, assoc_id: u32) -> Result<(), ConnectionError> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Dissociate { assoc_id };
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    async fn get_send_stream(&self) -> Result<SendStream, ConnectionError> {
        let send = self
            .conn
            .open_uni()
            .await
            .map_err(ConnectionError::from_quinn_connection_error)?;

        Ok(SendStream::new(send, self.stream_reg.as_ref().clone()))
    }

    async fn get_bi_stream(&self) -> Result<Stream, ConnectionError> {
        let (send, recv) = self
            .conn
            .open_bi()
            .await
            .map_err(ConnectionError::from_quinn_connection_error)?;

        let send = SendStream::new(send, self.stream_reg.as_ref().clone());
        let recv = RecvStream::new(recv, self.stream_reg.as_ref().clone());

        Ok(Stream::new(send, recv))
    }

    fn send_packet_to_datagram(
        &self,
        assoc_id: u32,
        addr: Address,
        pkt: Bytes,
    ) -> Result<(), ConnectionError> {
        let max_datagram_size = if let Some(size) = self.conn.max_datagram_size() {
            size
        } else {
            return Err(ConnectionError::DatagramDisabled);
        };

        let pkt_id = self.next_pkt_id.fetch_add(1, Ordering::SeqCst);
        let mut pkts = common::split_packet(pkt, &addr, max_datagram_size);
        let frag_total = pkts.len() as u8;

        let first_pkt = pkts.next().unwrap();
        let first_pkt_header = Command::Packet {
            assoc_id,
            pkt_id,
            frag_total,
            frag_id: 0,
            len: first_pkt.len() as u16,
            addr: Some(addr),
        };

        let mut buf = BytesMut::with_capacity(first_pkt_header.serialized_len() + first_pkt.len());
        first_pkt_header.write_to_buf(&mut buf);
        buf.extend_from_slice(&first_pkt);
        let buf = buf.freeze();

        self.conn
            .send_datagram(buf)
            .map_err(ConnectionError::from_quinn_send_datagram_error)?;

        for (id, pkt) in pkts.enumerate() {
            let pkt_header = Command::Packet {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id: id as u8 + 1,
                len: pkt.len() as u16,
                addr: None,
            };

            let mut buf = BytesMut::with_capacity(pkt_header.serialized_len() + pkt.len());
            pkt_header.write_to_buf(&mut buf);
            buf.extend_from_slice(&pkt);
            let buf = buf.freeze();

            self.conn
                .send_datagram(buf)
                .map_err(ConnectionError::from_quinn_send_datagram_error)?;
        }

        Ok(())
    }

    async fn send_packet_to_uni_stream(
        &self,
        assoc_id: u32,
        addr: Address,
        pkt: Bytes,
    ) -> Result<(), ConnectionError> {
        let mut send = self.get_send_stream().await?;

        let cmd = Command::Packet {
            assoc_id,
            pkt_id: self.next_pkt_id.fetch_add(1, Ordering::SeqCst),
            frag_total: 1,
            frag_id: 0,
            len: pkt.len() as u16,
            addr: Some(addr),
        };

        cmd.write_to(&mut send).await?;
        send.write_all(&pkt).await?;
        send.finish().await?;

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Tuic(TuicError),
    #[error("datagrams not supported by peer")]
    DatagramUnsupportedByPeer,
    #[error("datagram support disabled")]
    DatagramDisabled,
    #[error("datagram too large")]
    DatagramTooLarge,
}

impl ConnectionError {
    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }

    #[inline]
    fn from_quinn_send_datagram_error(err: QuinnSendDatagramError) -> Self {
        match err {
            QuinnSendDatagramError::UnsupportedByPeer => Self::DatagramUnsupportedByPeer,
            QuinnSendDatagramError::Disabled => Self::DatagramDisabled,
            QuinnSendDatagramError::TooLarge => Self::DatagramTooLarge,
            QuinnSendDatagramError::ConnectionLost(err) => Self::Io(IoError::from(err)),
        }
    }
}

impl From<TuicError> for ConnectionError {
    #[inline]
    fn from(err: TuicError) -> Self {
        match err {
            TuicError::Io(err) => Self::Io(err),
            err => Self::Tuic(err),
        }
    }
}

impl From<ConnectionError> for IoError {
    #[inline]
    fn from(err: ConnectionError) -> Self {
        match err {
            ConnectionError::Io(err) => Self::from(err),
            err => Self::new(ErrorKind::Other, err),
        }
    }
}
