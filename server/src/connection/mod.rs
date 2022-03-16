use futures_util::StreamExt;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError, Datagrams, IncomingBiStreams,
    IncomingUniStreams, NewConnection, VarInt,
};
use std::{sync::Arc, time::Duration};
use tokio::{sync::mpsc::Receiver as MpscReceiver, time};
use tuic_protocol::Address;

pub use self::{
    authenticate::{AuthenticateBroadcast, IsAuthenticated},
    udp_session::{
        RecvPacketReceiver, RecvPacketSender, SendPacketReceiver, SendPacketSender, UdpSessionMap,
    },
};

mod authenticate;
mod dispatch;
mod udp_session;

pub struct Connection {
    controller: QuinnConnection,
    udp_sessions: Arc<UdpSessionMap>,
    is_authenticated: IsAuthenticated,
    authenticate_broadcast: Arc<AuthenticateBroadcast>,
}

impl Connection {
    pub async fn handle_connection(conn: Connecting, expected_token_digest: [u8; 32]) {
        match conn.await {
            Ok(NewConnection {
                connection,
                uni_streams,
                bi_streams,
                datagrams,
                ..
            }) => {
                let (udp_sessions, recv_pkt_rx) = UdpSessionMap::new();
                let auth_bcast = Arc::new(AuthenticateBroadcast::new());

                let conn = Self {
                    controller: connection.clone(),
                    udp_sessions: Arc::new(udp_sessions),
                    is_authenticated: IsAuthenticated::new(connection, auth_bcast.clone()),
                    authenticate_broadcast: auth_bcast,
                };

                tokio::join!(
                    conn.handle_authentication_timeout(Duration::from_secs(3)),
                    conn.listen_uni_streams(uni_streams, expected_token_digest),
                    conn.listen_bi_streams(bi_streams),
                    conn.listen_datagrams(datagrams),
                    conn.listen_received_udp_packet(recv_pkt_rx)
                );
            }
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn listen_uni_streams(
        &self,
        mut uni_streams: IncomingUniStreams,
        expected_token_digest: [u8; 32],
    ) {
        while let Some(stream) = uni_streams.next().await {
            match stream {
                Ok(stream) => {
                    tokio::spawn(dispatch::handle_uni_stream(
                        stream,
                        self.controller.clone(),
                        self.udp_sessions.clone(),
                        expected_token_digest,
                        self.is_authenticated.clone(),
                        self.authenticate_broadcast.clone(),
                    ));
                }
                Err(err) => {
                    match err {
                        ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                        err => eprintln!("{err}"),
                    }
                    break;
                }
            }
        }
    }

    async fn listen_bi_streams(&self, mut bi_streams: IncomingBiStreams) {
        while let Some(stream) = bi_streams.next().await {
            match stream {
                Ok((send, recv)) => {
                    tokio::spawn(dispatch::handle_bi_stream(
                        send,
                        recv,
                        self.controller.clone(),
                        self.is_authenticated.clone(),
                    ));
                }
                Err(err) => {
                    match err {
                        ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                        err => eprintln!("{err}"),
                    }
                    break;
                }
            }
        }
    }

    async fn listen_datagrams(&self, datagrams: Datagrams) {}

    async fn listen_received_udp_packet(
        &self,
        mut recv_pkt_rx: MpscReceiver<(u32, Vec<u8>, Address)>,
    ) {
        while let Some((assoc_id, pkt, addr)) = recv_pkt_rx.recv().await {
            tokio::spawn(dispatch::handle_received_udp_packet(
                self.controller.clone(),
                assoc_id,
                pkt,
                addr,
            ));
        }
    }

    async fn handle_authentication_timeout(&self, timeout: Duration) {
        time::sleep(timeout).await;

        if !self.is_authenticated.check() {
            self.controller
                .close(VarInt::MAX, b"Authentication timeout");
            eprintln!("Authentication timeout");
            self.authenticate_broadcast.wake();
        }
    }
}
