use anyhow::{bail, Result};
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
    udp::{
        RecvPacketReceiver, RecvPacketSender, SendPacketReceiver, SendPacketSender, UdpSessionMap,
    },
};

mod authenticate;
mod handler;
mod stream;
mod udp;

#[derive(Clone)]
pub struct Connection {
    controller: QuinnConnection,
    udp_sessions: Arc<UdpSessionMap>,
    expected_token_digest: [u8; 32],
    is_authenticated: IsAuthenticated,
    authenticate_broadcast: Arc<AuthenticateBroadcast>,
}

impl Connection {
    pub async fn handle(conn: Connecting, exp_token_dgst: [u8; 32], auth_timeout: Duration) {
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
                    expected_token_digest: exp_token_dgst,
                    is_authenticated: IsAuthenticated::new(connection, auth_bcast.clone()),
                    authenticate_broadcast: auth_bcast,
                };

                let listen_uni_streams =
                    tokio::spawn(Self::listen_uni_streams(conn.clone(), uni_streams));
                let listen_bi_streams =
                    tokio::spawn(Self::listen_bi_streams(conn.clone(), bi_streams));
                let listen_datagrams =
                    tokio::spawn(Self::listen_datagrams(conn.clone(), datagrams));
                let listen_received_udp_packet =
                    tokio::spawn(Self::listen_received_udp_packet(conn.clone(), recv_pkt_rx));
                let handle_authentication_timeout =
                    tokio::spawn(Self::handle_authentication_timeout(conn, auth_timeout));

                let (
                    listen_uni_streams,
                    listen_bi_streams,
                    listen_datagrams,
                    listen_received_udp_packet,
                    handle_authentication_timeout,
                ) = unsafe {
                    tokio::try_join!(
                        listen_uni_streams,
                        listen_bi_streams,
                        listen_datagrams,
                        listen_received_udp_packet,
                        handle_authentication_timeout
                    )
                    .unwrap_unchecked()
                };

                let res = listen_uni_streams
                    .and(listen_bi_streams)
                    .and(listen_datagrams)
                    .and(listen_received_udp_packet)
                    .and(handle_authentication_timeout);

                match res {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            }
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn listen_uni_streams(self, mut uni_streams: IncomingUniStreams) -> Result<()> {
        while let Some(stream) = uni_streams.next().await {
            let stream = stream?;
            tokio::spawn(Self::process_uni_stream(self.clone(), stream));
        }

        Err(ConnectionError::LocallyClosed)?
    }

    async fn listen_bi_streams(self, mut bi_streams: IncomingBiStreams) -> Result<()> {
        while let Some(stream) = bi_streams.next().await {
            let (send, recv) = stream?;
            tokio::spawn(Self::process_bi_stream(self.clone(), send, recv));
        }

        Err(ConnectionError::LocallyClosed)?
    }

    async fn listen_datagrams(self, mut datagrams: Datagrams) -> Result<()> {
        while let Some(datagram) = datagrams.next().await {
            let datagram = datagram?;
            tokio::spawn(Self::process_datagram(self.clone(), datagram));
        }

        Err(ConnectionError::LocallyClosed)?
    }

    async fn listen_received_udp_packet(
        self,
        mut recv_pkt_rx: MpscReceiver<(u32, Vec<u8>, Address)>,
    ) -> Result<()> {
        while let Some((assoc_id, pkt, addr)) = recv_pkt_rx.recv().await {
            tokio::spawn(Self::process_received_udp_packet(
                self.clone(),
                assoc_id,
                pkt,
                addr,
            ));
        }

        Ok(())
    }

    async fn handle_authentication_timeout(self, timeout: Duration) -> Result<()> {
        time::sleep(timeout).await;

        if !self.is_authenticated.check() {
            self.controller
                .close(VarInt::MAX, b"Authentication timeout");

            self.authenticate_broadcast.wake();

            bail!("Authentication timeout");
        }

        Ok(())
    }
}
