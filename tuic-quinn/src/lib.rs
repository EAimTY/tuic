use self::{
    marshal::Marshal,
    side::Side,
    unmarshal::{Unmarshal, UnmarshalError},
};
use bytes::Bytes;
use futures_util::{io::Cursor, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use quinn::{
    Connection as QuinnConnection, ConnectionError, RecvStream, SendDatagramError, SendStream,
};
use std::{
    io::Error as IoError,
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;
use tuic::{
    model::{
        side::{Rx, Tx},
        AssembleError, Connect as ConnectModel, Connection as ConnectionModel,
        Packet as PacketModel,
    },
    protocol::{Address, Header},
};

mod marshal;
mod unmarshal;

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
    model: ConnectionModel<Vec<u8>>,
    _marker: Side,
}

impl<'conn, Side> Connection<'conn, Side> {
    pub async fn packet_native(
        &self,
        pkt: impl AsRef<[u8]>,
        assoc_id: u16,
        addr: Address,
    ) -> Result<(), Error> {
        let Some(max_pkt_size) = self.conn.max_datagram_size() else {
            return Err(Error::SendDatagram(SendDatagramError::Disabled));
        };

        let model = self.model.send_packet(assoc_id, addr, max_pkt_size);

        for (header, frag) in model.into_fragments(pkt) {
            let mut buf = Cursor::new(vec![0; header.len() + frag.len()]);
            header.marshal(&mut buf).await?;
            buf.write_all(frag).await.unwrap();
            self.conn.send_datagram(Bytes::from(buf.into_inner()))?;
        }

        Ok(())
    }

    pub async fn packet_quic(
        &self,
        pkt: impl AsRef<[u8]>,
        assoc_id: u16,
        addr: Address,
    ) -> Result<(), Error> {
        let model = self.model.send_packet(assoc_id, addr, u16::MAX as usize);
        let mut frags = model.into_fragments(pkt);
        let (header, frag) = frags.next().unwrap();
        assert!(frags.next().is_none());

        let mut send = self.conn.open_uni().await?;
        header.marshal(&mut send).await?;
        AsyncWriteExt::write_all(&mut send, frag).await?;

        Ok(())
    }

    async fn accept_packet_quic(
        &self,
        model: PacketModel<Rx, Vec<u8>>,
        mut recv: &mut RecvStream,
    ) -> Result<Option<(Vec<u8>, Address, u16)>, Error> {
        let mut buf = vec![0; *model.size() as usize];
        AsyncReadExt::read_exact(&mut recv, &mut buf).await?;
        let mut asm = Vec::new();

        Ok(model
            .assemble(buf)?
            .map(|pkt| pkt.assemble(&mut asm))
            .map(|(addr, assoc_id)| (asm, addr, assoc_id)))
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

    pub async fn connect(&self, addr: Address) -> Result<Connect, Error> {
        let (mut send, recv) = self.conn.open_bi().await?;
        let model = self.model.send_connect(addr);
        model.header().marshal(&mut send).await?;
        Ok(Connect::new(Side::Client(model), send, recv))
    }

    pub async fn handle_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        match Header::unmarshal(&mut recv).await? {
            Header::Authenticate(_) => Err(Error::BadCommand("authenticate")),
            Header::Connect(_) => Err(Error::BadCommand("connect")),
            Header::Packet(pkt) => {
                let model = self.model.recv_packet(pkt);
                Ok(Task::Packet(
                    self.accept_packet_quic(model, &mut recv).await?,
                ))
            }
            Header::Dissociate(_) => Err(Error::BadCommand("dissociate")),
            Header::Heartbeat(hb) => {
                let _ = self.model.recv_heartbeat(hb);
                Ok(Task::Heartbeat)
            }
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

    pub async fn handle_uni_stream(&self, mut recv: RecvStream) -> Result<Task, Error> {
        match Header::unmarshal(&mut recv).await? {
            Header::Authenticate(auth) => {
                let model = self.model.recv_authenticate(auth);
                Ok(Task::Authenticate(*model.token()))
            }
            Header::Connect(_) => Err(Error::BadCommand("connect")),
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
    Packet(Option<(Vec<u8>, Address, u16)>),
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
    Unmarshal(#[from] UnmarshalError),
    #[error(transparent)]
    Assemble(#[from] AssembleError),
    #[error("{0}")]
    BadCommand(&'static str),
}
