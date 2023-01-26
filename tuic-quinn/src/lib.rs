use self::{marshal::Marshal, side::Side};
use bytes::Bytes;
use futures_util::{io::Cursor, AsyncRead, AsyncWrite, AsyncWriteExt};
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
        Connect as ConnectModel, Connection as ConnectionModel,
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
    model: ConnectionModel<Bytes>,
    _marker: Side,
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
                let Header::Connect(connect) = model.header() else { unreachable!() };
                &connect.addr()
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

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
}
