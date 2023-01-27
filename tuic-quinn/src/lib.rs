use self::side::Side;
use bytes::{BufMut, Bytes};
use futures_util::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use quinn::{
    Connection as QuinnConnection, ConnectionError, RecvStream, SendDatagramError, SendStream,
};
use std::{
    io::{Cursor, Error as IoError},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use thiserror::Error;
use tuic::{
    model::{
        side::{Rx, Tx},
        AssembleError, Connect as ConnectModel, Connection as ConnectionModel,
        Packet as PacketModel,
    },
    Address, Header, UnmarshalError,
};

pub mod side {
    pub struct Client;
    pub struct Server;

    pub(super) enum Side<C, S> {
        Client(C),
        Server(S),
    }
}

pub struct Connection<'conn, Side> {
    conn: &'conn QuinnConnection,
    model: ConnectionModel<Bytes>,
    _marker: Side,
}

impl<'conn, Side> Connection<'conn, Side> {
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
            let mut buf = vec![0; header.len() + frag.len()];
            header.write(&mut buf);
            buf.put_slice(frag);
            self.conn.send_datagram(Bytes::from(buf))?;
        }

        Ok(())
    }

    pub async fn packet_quic(
        &self,
        pkt: impl AsRef<[u8]>,
        addr: Address,
        assoc_id: u16,
    ) -> Result<(), Error> {
        let model = self.model.send_packet(assoc_id, addr, u16::MAX as usize);
        let mut frags = model.into_fragments(pkt);
        let (header, frag) = frags.next().unwrap();
        assert!(frags.next().is_none());

        let mut send = self.conn.open_uni().await?;
        header.async_marshal(&mut send).await?;
        AsyncWriteExt::write_all(&mut send, frag).await?;
        send.close().await?;

        Ok(())
    }

    pub fn collect_garbage(&self, timeout: Duration) {
        self.model.collect_garbage(timeout);
    }

    async fn accept_packet_quic(
        &self,
        model: PacketModel<Rx, Bytes>,
        mut recv: &mut RecvStream,
    ) -> Result<Option<(Bytes, Address, u16)>, Error> {
        let mut buf = vec![0; model.size() as usize];
        AsyncReadExt::read_exact(&mut recv, &mut buf).await?;
        let mut asm = Vec::new();

        Ok(model
            .assemble(Bytes::from(buf))?
            .map(|pkt| pkt.assemble(&mut asm))
            .map(|(addr, assoc_id)| (Bytes::from(asm), addr, assoc_id)))
    }

    fn accept_packet_native(
        &self,
        model: PacketModel<Rx, Bytes>,
        data: Bytes,
    ) -> Result<Option<(Bytes, Address, u16)>, Error> {
        let mut asm = Vec::new();

        Ok(model
            .assemble(data)?
            .map(|pkt| pkt.assemble(&mut asm))
            .map(|(addr, assoc_id)| (Bytes::from(asm), addr, assoc_id)))
    }
}

impl<'conn> Connection<'conn, side::Client> {
    pub fn new(conn: &'conn QuinnConnection) -> Self {
        Self {
            conn,
            model: ConnectionModel::new(),
            _marker: side::Client,
        }
    }

    pub async fn authenticate(&self, token: [u8; 8]) -> Result<(), Error> {
        let mut send = self.conn.open_uni().await?;
        let model = self.model.send_authenticate(token);
        model.header().async_marshal(&mut send).await?;
        send.close().await?;
        Ok(())
    }

    pub async fn connect(&self, addr: Address) -> Result<Connect, Error> {
        let (mut send, recv) = self.conn.open_bi().await?;
        let model = self.model.send_connect(addr);
        model.header().async_marshal(&mut send).await?;
        Ok(Connect::new(Side::Client(model), send, recv))
    }

    pub async fn heartbeat(&self) -> Result<(), Error> {
        let model = self.model.send_heartbeat();
        let mut buf = Vec::with_capacity(model.header().len());
        model.header().async_marshal(&mut buf).await.unwrap();
        self.conn.send_datagram(Bytes::from(buf))?;
        Ok(())
    }

    pub async fn accept_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalUniStream(err, recv)),
        };

        match header {
            Header::Authenticate(_) => Err(Error::BadCommandUniStream("authenticate", recv)),
            Header::Connect(_) => Err(Error::BadCommandUniStream("connect", recv)),
            Header::Packet(pkt) => {
                let model = self.model.recv_packet(pkt);
                Ok(Task::Packet(
                    self.accept_packet_quic(model, &mut recv).await?,
                ))
            }
            Header::Dissociate(_) => Err(Error::BadCommandUniStream("dissociate", recv)),
            Header::Heartbeat(_) => Err(Error::BadCommandUniStream("heartbeat", recv)),
            _ => unreachable!(),
        }
    }

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
                let model = self.model.recv_packet(pkt);
                let pos = dg.position() as usize;
                let buf = dg.into_inner().slice(pos..pos + model.size() as usize);
                Ok(Task::Packet(self.accept_packet_native(model, buf)?))
            }
            Header::Dissociate(_) => Err(Error::BadCommandDatagram("dissociate", dg.into_inner())),
            Header::Heartbeat(_) => Err(Error::BadCommandDatagram("heartbeat", dg.into_inner())),
            _ => unreachable!(),
        }
    }
}

impl<'conn> Connection<'conn, side::Server> {
    pub fn new(conn: &'conn QuinnConnection) -> Self {
        Self {
            conn,
            model: ConnectionModel::new(),
            _marker: side::Server,
        }
    }

    pub async fn accept_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        let header = match Header::async_unmarshal(&mut recv).await {
            Ok(header) => header,
            Err(err) => return Err(Error::UnmarshalUniStream(err, recv)),
        };

        match header {
            Header::Authenticate(auth) => {
                let model = self.model.recv_authenticate(auth);
                Ok(Task::Authenticate(model.token()))
            }
            Header::Connect(_) => Err(Error::BadCommandUniStream("connect", recv)),
            Header::Packet(pkt) => {
                let model = self.model.recv_packet(pkt);
                Ok(Task::Packet(
                    self.accept_packet_quic(model, &mut recv).await?,
                ))
            }
            Header::Dissociate(dissoc) => {
                let _ = self.model.recv_dissociate(dissoc);
                Ok(Task::Dissociate)
            }
            Header::Heartbeat(_) => Err(Error::BadCommandUniStream("heartbeat", recv)),
            _ => unreachable!(),
        }
    }

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
                let model = self.model.recv_packet(pkt);
                let pos = dg.position() as usize;
                let buf = dg.into_inner().slice(pos..pos + model.size() as usize);
                Ok(Task::Packet(self.accept_packet_native(model, buf)?))
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

    pub fn addr(&self) -> &Address {
        match &self.model {
            Side::Client(model) => {
                let Header::Connect(conn) = model.header() else { unreachable!() };
                &conn.addr()
            }
            Side::Server(model) => model.addr(),
        }
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

#[non_exhaustive]
pub enum Task {
    Authenticate([u8; 8]),
    Connect(Connect),
    Packet(Option<(Bytes, Address, u16)>),
    Dissociate,
    Heartbeat,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
    #[error(transparent)]
    Assemble(#[from] AssembleError),
    #[error("error unmarshaling uni_stream: {0}")]
    UnmarshalUniStream(UnmarshalError, RecvStream),
    #[error("error unmarshaling bi_stream: {0}")]
    UnmarshalBiStream(UnmarshalError, SendStream, RecvStream),
    #[error("error unmarshaling datagram: {0}")]
    UnmarshalDatagram(UnmarshalError, Bytes),
    #[error("bad command `{0}` from uni_stream")]
    BadCommandUniStream(&'static str, RecvStream),
    #[error("bad command `{0}` from bi_stream")]
    BadCommandBiStream(&'static str, SendStream, RecvStream),
    #[error("bad command `{0}` from datagram")]
    BadCommandDatagram(&'static str, Bytes),
}
