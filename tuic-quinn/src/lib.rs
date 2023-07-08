#![doc = include_str!("../README.md")]

use self::side::Side;
use bytes::{BufMut, Bytes, BytesMut};
use futures_util::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use quinn::{
    Connection as QuinnConnection, ConnectionError, RecvStream, SendDatagramError, SendStream,
    UnknownStream, VarInt,
};
use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    io::{Cursor, Error as IoError},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use thiserror::Error;
use tuic::{
    model::{
        side::{Rx, Tx},
        AssembleError, Authenticate as AuthenticateModel, Connect as ConnectModel,
        Connection as ConnectionModel, KeyingMaterialExporter as KeyingMaterialExporterImpl,
        Packet as PacketModel,
    },
    Address, Header, UnmarshalError,
};
use uuid::Uuid;

pub mod side {
    //! Side marker types for a connection.

    #[derive(Clone, Debug)]
    pub struct Client;
    #[derive(Clone, Debug)]
    pub struct Server;

    pub(super) enum Side<C, S> {
        Client(C),
        Server(S),
    }
}

/// The TUIC Connection.
///
/// This struct takes a clone of `quinn::Connection` for performing TUIC operations.
///
/// See more details about the TUIC protocol at [SPEC.md](https://github.com/EAimTY/tuic/blob/dev/tuic/SPEC.md)
#[derive(Clone)]
pub struct Connection<Side> {
    conn: QuinnConnection,
    model: ConnectionModel<Bytes>,
    _marker: Side,
}

impl<Side> Connection<Side> {
    /// Sends a `Packet` using UDP relay mode `native`.
    pub fn packet_native(
        &self,
        pkt: impl AsRef<[u8]>,
        addr: Address,
        assoc_id: u16,
    ) -> Result<(), Error> {
        let Some(max_pkt_size) = self.conn.max_datagram_size() else {
            return Err(Error::SendDatagram(SendDatagramError::Disabled));
        };

        let model = self.model.send_packet(assoc_id, addr, max_pkt_size);

        for (header, frag) in model.into_fragments(pkt) {
            let mut buf = BytesMut::with_capacity(header.len() + frag.len());
            header.write(&mut buf);
            buf.put_slice(frag);
            self.conn.send_datagram(Bytes::from(buf))?;
        }

        Ok(())
    }

    /// Sends a `Packet` using UDP relay mode `quic`.
    pub async fn packet_quic(
        &self,
        pkt: impl AsRef<[u8]>,
        addr: Address,
        assoc_id: u16,
    ) -> Result<(), Error> {
        let model = self.model.send_packet(assoc_id, addr, u16::MAX as usize);

        for (header, frag) in model.into_fragments(pkt) {
            let mut send = self.conn.open_uni().await?;
            header.async_marshal(&mut send).await?;
            AsyncWriteExt::write_all(&mut send, frag).await?;
            send.close().await?;
        }

        Ok(())
    }

    /// Returns the number of `Connect` tasks
    pub fn task_connect_count(&self) -> usize {
        self.model.task_connect_count()
    }

    /// Returns the number of active UDP sessions
    pub fn task_associate_count(&self) -> usize {
        self.model.task_associate_count()
    }

    /// Removes packet fragments that can not be reassembled within the specified timeout
    pub fn collect_garbage(&self, timeout: Duration) {
        self.model.collect_garbage(timeout);
    }

    fn keying_material_exporter(&self) -> KeyingMaterialExporter {
        KeyingMaterialExporter(self.conn.clone())
    }
}

impl Connection<side::Client> {
    /// Creates a new client side `Connection`.
    pub fn new(conn: QuinnConnection) -> Self {
        Self {
            conn,
            model: ConnectionModel::new(),
            _marker: side::Client,
        }
    }

    /// Sends an `Authenticate` command.
    pub async fn authenticate(&self, uuid: Uuid, password: impl AsRef<[u8]>) -> Result<(), Error> {
        let model = self
            .model
            .send_authenticate(uuid, password, &self.keying_material_exporter());

        let mut send = self.conn.open_uni().await?;
        model.header().async_marshal(&mut send).await?;
        send.close().await?;
        Ok(())
    }

    /// Sends a `Connect` command.
    pub async fn connect(&self, addr: Address) -> Result<Connect, Error> {
        let model = self.model.send_connect(addr);
        let (mut send, recv) = self.conn.open_bi().await?;
        model.header().async_marshal(&mut send).await?;
        Ok(Connect::new(Side::Client(model), send, recv))
    }

    /// Sends a `Dissociate` command.
    pub async fn dissociate(&self, assoc_id: u16) -> Result<(), Error> {
        let model = self.model.send_dissociate(assoc_id);
        let mut send = self.conn.open_uni().await?;
        model.header().async_marshal(&mut send).await?;
        send.close().await?;
        Ok(())
    }

    /// Sends a `Heartbeat` command.
    pub async fn heartbeat(&self) -> Result<(), Error> {
        let model = self.model.send_heartbeat();
        let mut buf = Vec::with_capacity(model.header().len());
        model.header().async_marshal(&mut buf).await.unwrap();
        self.conn.send_datagram(Bytes::from(buf))?;
        Ok(())
    }

    /// Try to parse a `quinn::RecvStream` as a TUIC command.
    ///
    /// The `quinn::RecvStream` should be accepted by `quinn::Connection::accept_uni()` from the same `quinn::Connection`.
    pub async fn accept_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalUniStream(err, recv)),
        };

        match header {
            Header::Authenticate(_) => Err(Error::BadCommandUniStream("authenticate", recv)),
            Header::Connect(_) => Err(Error::BadCommandUniStream("connect", recv)),
            Header::Packet(pkt) => {
                let assoc_id = pkt.assoc_id();
                let pkt_id = pkt.pkt_id();
                self.model
                    .recv_packet(pkt)
                    .map_or(Err(Error::InvalidUdpSession(assoc_id, pkt_id)), |pkt| {
                        Ok(Task::Packet(Packet::new(pkt, PacketSource::Quic(recv))))
                    })
            }
            Header::Dissociate(_) => Err(Error::BadCommandUniStream("dissociate", recv)),
            Header::Heartbeat(_) => Err(Error::BadCommandUniStream("heartbeat", recv)),
            _ => unreachable!(),
        }
    }

    /// Try to parse a pair of `quinn::SendStream` and `quinn::RecvStream` as a TUIC command.
    ///
    /// The pair of stream should be accepted by `quinn::Connection::accept_bi()` from the same `quinn::Connection`.
    pub async fn accept_bi_stream(
        &self,
        send: SendStream,
        mut recv: RecvStream,
    ) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalBiStream(err, send, recv)),
        };

        match header {
            Header::Authenticate(_) => Err(Error::BadCommandBiStream("authenticate", send, recv)),
            Header::Connect(_) => Err(Error::BadCommandBiStream("connect", send, recv)),
            Header::Packet(_) => Err(Error::BadCommandBiStream("packet", send, recv)),
            Header::Dissociate(_) => Err(Error::BadCommandBiStream("dissociate", send, recv)),
            Header::Heartbeat(_) => Err(Error::BadCommandBiStream("heartbeat", send, recv)),
            _ => unreachable!(),
        }
    }

    /// Try to parse a QUIC Datagram as a TUIC command.
    ///
    /// The Datagram should be accepted by `quinn::Connection::read_datagram()` from the same `quinn::Connection`.
    pub fn accept_datagram(&self, dg: Bytes) -> Result<Task, Error> {
        let mut dg = Cursor::new(dg);

        let header = match Header::unmarshal(&mut dg) {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalDatagram(err, dg.into_inner())),
        };

        match header {
            Header::Authenticate(_) => {
                Err(Error::BadCommandDatagram("authenticate", dg.into_inner()))
            }
            Header::Connect(_) => Err(Error::BadCommandDatagram("connect", dg.into_inner())),
            Header::Packet(pkt) => {
                let assoc_id = pkt.assoc_id();
                let pkt_id = pkt.pkt_id();
                if let Some(pkt) = self.model.recv_packet(pkt) {
                    let pos = dg.position() as usize;
                    let mut buf = dg.into_inner();
                    if (pos + pkt.size() as usize) <= buf.len() {
                        buf = buf.slice(pos..pos + pkt.size() as usize);
                        Ok(Task::Packet(Packet::new(pkt, PacketSource::Native(buf))))
                    } else {
                        Err(Error::PayloadLength(pkt.size() as usize, buf.len() - pos))
                    }
                } else {
                    Err(Error::InvalidUdpSession(assoc_id, pkt_id))
                }
            }
            Header::Dissociate(_) => Err(Error::BadCommandDatagram("dissociate", dg.into_inner())),
            Header::Heartbeat(_) => Err(Error::BadCommandDatagram("heartbeat", dg.into_inner())),
            _ => unreachable!(),
        }
    }
}

impl Connection<side::Server> {
    /// Creates a new server side `Connection`.
    pub fn new(conn: QuinnConnection) -> Self {
        Self {
            conn,
            model: ConnectionModel::new(),
            _marker: side::Server,
        }
    }

    /// Try to parse a `quinn::RecvStream` as a TUIC command.
    ///
    /// The `quinn::RecvStream` should be accepted by `quinn::Connection::accept_uni()` from the same `quinn::Connection`.
    pub async fn accept_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalUniStream(err, recv)),
        };

        match header {
            Header::Authenticate(auth) => {
                let model = self.model.recv_authenticate(auth);
                Ok(Task::Authenticate(Authenticate::new(
                    model,
                    self.keying_material_exporter(),
                )))
            }
            Header::Connect(_) => Err(Error::BadCommandUniStream("connect", recv)),
            Header::Packet(pkt) => {
                let model = self.model.recv_packet_unrestricted(pkt);
                Ok(Task::Packet(Packet::new(model, PacketSource::Quic(recv))))
            }
            Header::Dissociate(dissoc) => {
                let model = self.model.recv_dissociate(dissoc);
                Ok(Task::Dissociate(model.assoc_id()))
            }
            Header::Heartbeat(_) => Err(Error::BadCommandUniStream("heartbeat", recv)),
            _ => unreachable!(),
        }
    }

    /// Try to parse a pair of `quinn::SendStream` and `quinn::RecvStream` as a TUIC command.
    ///
    /// The pair of stream should be accepted by `quinn::Connection::accept_bi()` from the same `quinn::Connection`.
    pub async fn accept_bi_stream(
        &self,
        send: SendStream,
        mut recv: RecvStream,
    ) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalBiStream(err, send, recv)),
        };

        match header {
            Header::Authenticate(_) => Err(Error::BadCommandBiStream("authenticate", send, recv)),
            Header::Connect(conn) => {
                let model = self.model.recv_connect(conn);
                Ok(Task::Connect(Connect::new(Side::Server(model), send, recv)))
            }
            Header::Packet(_) => Err(Error::BadCommandBiStream("packet", send, recv)),
            Header::Dissociate(_) => Err(Error::BadCommandBiStream("dissociate", send, recv)),
            Header::Heartbeat(_) => Err(Error::BadCommandBiStream("heartbeat", send, recv)),
            _ => unreachable!(),
        }
    }

    /// Try to parse a QUIC Datagram as a TUIC command.
    ///
    /// The Datagram should be accepted by `quinn::Connection::read_datagram()` from the same `quinn::Connection`.
    pub fn accept_datagram(&self, dg: Bytes) -> Result<Task, Error> {
        let mut dg = Cursor::new(dg);

        let header = match Header::unmarshal(&mut dg) {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalDatagram(err, dg.into_inner())),
        };

        match header {
            Header::Authenticate(_) => {
                Err(Error::BadCommandDatagram("authenticate", dg.into_inner()))
            }
            Header::Connect(_) => Err(Error::BadCommandDatagram("connect", dg.into_inner())),
            Header::Packet(pkt) => {
                let model = self.model.recv_packet_unrestricted(pkt);
                let pos = dg.position() as usize;
                let buf = dg.into_inner().slice(pos..pos + model.size() as usize);
                Ok(Task::Packet(Packet::new(model, PacketSource::Native(buf))))
            }
            Header::Dissociate(_) => Err(Error::BadCommandDatagram("dissociate", dg.into_inner())),
            Header::Heartbeat(hb) => {
                let _ = self.model.recv_heartbeat(hb);
                Ok(Task::Heartbeat)
            }
            _ => unreachable!(),
        }
    }
}

impl<Side> Debug for Connection<Side> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Connection")
            .field("conn", &self.conn)
            .field("model", &self.model)
            .finish()
    }
}

/// A received `Authenticate` command.
#[derive(Debug)]
pub struct Authenticate {
    model: AuthenticateModel<Rx>,
    exporter: KeyingMaterialExporter,
}

impl Authenticate {
    fn new(model: AuthenticateModel<Rx>, exporter: KeyingMaterialExporter) -> Self {
        Self { model, exporter }
    }

    /// The UUID of the client.
    pub fn uuid(&self) -> Uuid {
        self.model.uuid()
    }

    /// The hashed token.
    pub fn token(&self) -> [u8; 32] {
        self.model.token()
    }

    /// Validates if the given password is matching the hashed token.
    pub fn validate(&self, password: impl AsRef<[u8]>) -> bool {
        self.model.is_valid(password, &self.exporter)
    }
}

/// A received `Connect` command.
pub struct Connect {
    model: Side<ConnectModel<Tx>, ConnectModel<Rx>>,
    send: SendStream,
    recv: RecvStream,
}

impl Connect {
    fn new(
        model: Side<ConnectModel<Tx>, ConnectModel<Rx>>,
        send: SendStream,
        recv: RecvStream,
    ) -> Self {
        Self { model, send, recv }
    }

    /// Returns the `Connect` address
    pub fn addr(&self) -> &Address {
        match &self.model {
            Side::Client(model) => {
                let Header::Connect(conn) = model.header() else { unreachable!() };
                conn.addr()
            }
            Side::Server(model) => model.addr(),
        }
    }

    /// Immediately closes the `Connect` streams with the given error code. Returns the result of closing the send and receive streams, respectively.
    pub fn reset(
        &mut self,
        error_code: VarInt,
    ) -> (Result<(), UnknownStream>, Result<(), UnknownStream>) {
        let send_res = self.send.reset(error_code);
        let recv_res = self.recv.stop(error_code);
        (send_res, recv_res)
    }
}

impl AsyncRead for Connect {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        AsyncRead::poll_read(Pin::new(&mut self.get_mut().recv), cx, buf)
    }
}

impl AsyncWrite for Connect {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        AsyncWrite::poll_write(Pin::new(&mut self.get_mut().send), cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.get_mut().send), cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        AsyncWrite::poll_close(Pin::new(&mut self.get_mut().send), cx)
    }
}

impl Debug for Connect {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let model = match &self.model {
            Side::Client(model) => model as &dyn Debug,
            Side::Server(model) => model as &dyn Debug,
        };

        f.debug_struct("Connect")
            .field("model", model)
            .field("send", &self.send)
            .field("recv", &self.recv)
            .finish()
    }
}

/// A received `Packet` command.
#[derive(Debug)]
pub struct Packet {
    model: PacketModel<Rx, Bytes>,
    src: PacketSource,
}

#[derive(Debug)]
enum PacketSource {
    Quic(RecvStream),
    Native(Bytes),
}

impl Packet {
    fn new(model: PacketModel<Rx, Bytes>, src: PacketSource) -> Self {
        Self { src, model }
    }

    /// Returns the UDP session ID
    pub fn assoc_id(&self) -> u16 {
        self.model.assoc_id()
    }

    /// Returns the packet ID
    pub fn pkt_id(&self) -> u16 {
        self.model.pkt_id()
    }

    /// Returns the fragment ID
    pub fn frag_id(&self) -> u8 {
        self.model.frag_id()
    }

    /// Returns the total number of fragments
    pub fn frag_total(&self) -> u8 {
        self.model.frag_total()
    }

    /// Whether the packet is from UDP relay mode `quic`
    pub fn is_from_quic(&self) -> bool {
        matches!(self.src, PacketSource::Quic(_))
    }

    /// Whether the packet is from UDP relay mode `native`
    pub fn is_from_native(&self) -> bool {
        matches!(self.src, PacketSource::Native(_))
    }

    /// Accepts the packet payload. If the packet is fragmented and not yet fully assembled, `Ok(None)` is returned.
    pub async fn accept(self) -> Result<Option<(Bytes, Address, u16)>, Error> {
        let pkt = match self.src {
            PacketSource::Quic(mut recv) => {
                let mut buf = vec![0; self.model.size() as usize];
                AsyncReadExt::read_exact(&mut recv, &mut buf).await?;
                Bytes::from(buf)
            }
            PacketSource::Native(pkt) => pkt,
        };

        let mut asm = Vec::new();

        Ok(self
            .model
            .assemble(pkt)?
            .map(|pkt| pkt.assemble(&mut asm))
            .map(|(addr, assoc_id)| (Bytes::from(asm), addr, assoc_id)))
    }
}

/// Type of tasks that can be received.
#[non_exhaustive]
#[derive(Debug)]
pub enum Task {
    Authenticate(Authenticate),
    Connect(Connect),
    Packet(Packet),
    Dissociate(u16),
    Heartbeat,
}

#[derive(Debug)]
struct KeyingMaterialExporter(QuinnConnection);

impl KeyingMaterialExporterImpl for KeyingMaterialExporter {
    fn export_keying_material(&self, label: &[u8], context: &[u8]) -> [u8; 32] {
        let mut buf = [0; 32];
        self.0
            .export_keying_material(&mut buf, label, context)
            .unwrap();
        buf
    }
}

/// Errors that can occur when processing a task.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
    #[error("expecting payload length {0} but got {1}")]
    PayloadLength(usize, usize),
    #[error("packet {1:#06x} on invalid udp session {0:#06x}")]
    InvalidUdpSession(u16, u16),
    #[error(transparent)]
    Assemble(#[from] AssembleError),
    #[error("error unmarshalling uni_stream: {0}")]
    UnmarshalUniStream(UnmarshalError, RecvStream),
    #[error("error unmarshalling bi_stream: {0}")]
    UnmarshalBiStream(UnmarshalError, SendStream, RecvStream),
    #[error("error unmarshalling datagram: {0}")]
    UnmarshalDatagram(UnmarshalError, Bytes),
    #[error("bad command `{0}` from uni_stream")]
    BadCommandUniStream(&'static str, RecvStream),
    #[error("bad command `{0}` from bi_stream")]
    BadCommandBiStream(&'static str, SendStream, RecvStream),
    #[error("bad command `{0}` from datagram")]
    BadCommandDatagram(&'static str, Bytes),
}
