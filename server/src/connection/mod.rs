use futures_util::StreamExt;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError, Datagrams, IncomingBiStreams,
    IncomingUniStreams, NewConnection,
};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use tokio::sync::mpsc::Receiver as MpscReceiver;
use tuic_protocol::Address;

pub use self::udp_session::{
    RecvPacketReceiver, RecvPacketSender, SendPacketReceiver, SendPacketSender, UdpSessionMap,
};

mod dispatch;
mod udp_session;

pub struct Connection {
    controller: QuinnConnection,
    udp_sessions: Arc<UdpSessionMap>,
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
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

                let conn = Self {
                    controller: connection,
                    udp_sessions: Arc::new(udp_sessions),
                    is_authenticated: Arc::new(AtomicBool::new(false)),
                    create_time: Instant::now(),
                };

                tokio::join!(
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
                        self.create_time,
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
                        self.create_time,
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
}
