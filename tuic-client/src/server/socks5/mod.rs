use crate::{connection::ChannelMessage, ClientError, Config, Connection};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncWriteExt,
    net::{tcp, TcpListener, TcpStream},
    sync::mpsc,
};

mod convert;
mod protocol;

struct Socks5Connection {
    stream: TcpStream,
    channel: Arc<mpsc::Sender<ChannelMessage>>,
}

impl Socks5Connection {
    fn new(stream: TcpStream, channel: &Arc<mpsc::Sender<ChannelMessage>>) -> Self {
        Self {
            stream,
            channel: channel.clone(),
        }
    }

    async fn process(mut self) {
        tokio::spawn(async move {
            self.handshake().await.unwrap();

            let tcp_req = protocol::ConnectRequest::read_from(&mut self.stream)
                .await
                .unwrap();

            if let Ok((mut send, mut recv)) = self.handle_request(tcp_req).await {
                let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();

                let addr = listener.local_addr().unwrap();
                let tcp_res =
                    protocol::ConnectResponse::new(protocol::Reply::Succeeded, addr.into());
                tcp_res.write_to(&mut self.stream).await.unwrap();
                self.stream.shutdown().await.unwrap();

                let (stream, _) = listener.accept().await.unwrap();
                let (mut local_recv, mut local_send) = stream.into_split();

                self.forward(&mut send, &mut recv, &mut local_send, &mut local_recv)
                    .await;
            } else {
                let tcp_res = protocol::ConnectResponse::new(
                    protocol::Reply::GeneralFailure,
                    SocketAddr::from(([0, 0, 0, 0], 0)).into(),
                );
                tcp_res.write_to(&mut self.stream).await.unwrap();
                self.stream.shutdown().await.unwrap();
            }
        });
    }

    async fn forward(
        &self,
        remote_send: &mut quinn::SendStream,
        remote_recv: &mut quinn::RecvStream,
        local_send: &mut tcp::OwnedWriteHalf,
        local_recv: &mut tcp::OwnedReadHalf,
    ) {
        let remote_to_local = tokio::io::copy(remote_recv, local_send);
        let local_to_remote = tokio::io::copy(local_recv, remote_send);
        let _ = tokio::try_join!(remote_to_local, local_to_remote);
    }

    async fn handshake(&mut self) -> Result<(), ClientError> {
        let hs_req = protocol::HandshakeRequest::read_from(&mut self.stream)
            .await
            .unwrap();
        if hs_req
            .methods
            .contains(&protocol::handshake::SOCKS5_AUTH_METHOD_NONE)
        {
            let hs_res =
                protocol::HandshakeResponse::new(protocol::handshake::SOCKS5_AUTH_METHOD_NONE);
            hs_res.write_to(&mut self.stream).await.unwrap();
        } else {
            return Err(ClientError::Socks5AuthFailed);
        }

        Ok(())
    }

    async fn handle_request(
        &self,
        tcp_req: protocol::ConnectRequest,
    ) -> Result<(quinn::SendStream, quinn::RecvStream), ClientError> {
        let conn = self.get_tuic_connection().await.unwrap();
        if conn.handshake().await.is_err() {
            return Err(ClientError::AuthFailed);
        }

        let (mut send, mut recv) = conn.open_bi().await.unwrap();

        let tuic_connect_req = tuic_protocol::ConnectRequest::from(tcp_req);
        tuic_connect_req.write_to(&mut send).await.unwrap();

        let _tuic_connect_res = tuic_protocol::ConnectResponse::read_from(&mut recv)
            .await
            .unwrap();

        Ok((send, recv))
    }

    async fn get_tuic_connection(&self) -> Result<Connection, ClientError> {
        let (get_conn_msg, conn_receiver) = ChannelMessage::get_connection();
        self.channel
            .send(get_conn_msg)
            .await
            .map_err(|_| ())
            .unwrap();
        let conn = conn_receiver.await.map_err(|_| ()).unwrap();

        Ok(conn)
    }
}

pub struct Socks5Server {
    channel: Arc<mpsc::Sender<ChannelMessage>>,
}

impl Socks5Server {
    pub fn new(_config: &Config, channel: mpsc::Sender<ChannelMessage>) -> Self {
        Self {
            channel: Arc::new(channel),
        }
    }

    pub async fn run(&self) -> Result<(), ClientError> {
        let socks5_listener = TcpListener::bind("0.0.0.0:8887").await.unwrap();

        while let Ok((stream, _)) = socks5_listener.accept().await {
            let socks5_conn = Socks5Connection::new(stream, &self.channel);
            socks5_conn.process().await;
        }

        Ok(())
    }
}
