use super::{Connection, UdpRelayMode};
use futures_util::StreamExt;
use quinn::{ConnectionError, Datagrams, IncomingUniStreams};
use std::sync::Arc;
use tokio::sync::oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender};

pub async fn listen_incoming(
    mut next_incoming_rx: UdpRelayMode<Receiver<Datagrams>, Receiver<IncomingUniStreams>>,
) {
    loop {
        let (conn, incoming);
        (conn, incoming, next_incoming_rx) = match next_incoming_rx {
            UdpRelayMode::Native(incoming_rx) => {
                let (conn, incoming, next_incoming_rx) = incoming_rx.next().await;
                (
                    conn,
                    UdpRelayMode::Native(incoming),
                    UdpRelayMode::Native(next_incoming_rx),
                )
            }
            UdpRelayMode::Quic(incoming_rx) => {
                let (conn, incoming, next_incoming_rx) = incoming_rx.next().await;
                (
                    conn,
                    UdpRelayMode::Quic(incoming),
                    UdpRelayMode::Quic(next_incoming_rx),
                )
            }
        };

        let err = match incoming {
            UdpRelayMode::Native(mut incoming) => loop {
                let pkt = match incoming.next().await {
                    Some(Ok(pkt)) => pkt,
                    Some(Err(err)) => break err,
                    None => break ConnectionError::LocallyClosed,
                };

                // TODO: process datagram
                tokio::spawn(async move {});
            },
            UdpRelayMode::Quic(mut uni) => loop {
                let recv = match uni.next().await {
                    Some(Ok(recv)) => recv,
                    Some(Err(err)) => break err,
                    None => break ConnectionError::LocallyClosed,
                };

                // TODO: process stream
                tokio::spawn(async move {});
            },
        };

        match err {
            ConnectionError::LocallyClosed | ConnectionError::TimedOut => {}
            err => log::error!("[relay] [connection] {err}"),
        }

        conn.set_closed();
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = oneshot::channel();
    (Sender(tx), Receiver(rx))
}

pub struct Sender<T>(OneshotSender<(Connection, T, Receiver<T>)>);

impl<T> Sender<T> {
    pub fn send(self, conn: Connection, incoming: T, next_incoming_rx: Receiver<T>) {
        // safety: the receiver must not be dropped before
        let _ = self.0.send((conn, incoming, next_incoming_rx));
    }
}

pub struct Receiver<T>(OneshotReceiver<(Connection, T, Self)>);

impl<T> Receiver<T> {
    async fn next(self) -> (Connection, T, Self) {
        // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
        self.0.await.unwrap()
    }
}
