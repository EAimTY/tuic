use super::{connection::IsClosed, UdpRelayMode};
use futures_util::StreamExt;
use quinn::{ConnectionError, Datagrams, IncomingUniStreams};
use std::sync::Arc;
use tokio::sync::oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender};

pub async fn listen_incoming(
    mut next_incoming_rx: UdpRelayMode<Receiver<Datagrams>, Receiver<IncomingUniStreams>>,
) {
    loop {
        let (incoming, is_closed);
        (incoming, next_incoming_rx, is_closed) = match next_incoming_rx {
            UdpRelayMode::Native(incoming_rx) => {
                let (incoming, next_incoming_rx, is_closed) = incoming_rx.next().await;
                (
                    UdpRelayMode::Native(incoming),
                    UdpRelayMode::Native(next_incoming_rx),
                    is_closed,
                )
            }
            UdpRelayMode::Quic(incoming_rx) => {
                let (incoming, next_incoming_rx, is_closed) = incoming_rx.next().await;
                (
                    UdpRelayMode::Quic(incoming),
                    UdpRelayMode::Quic(next_incoming_rx),
                    is_closed,
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

        is_closed.set_closed();
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = oneshot::channel();
    (Sender(tx), Receiver(rx))
}

pub struct Sender<T>(OneshotSender<(T, Receiver<T>, IsClosed)>);

impl<T> Sender<T> {
    pub fn send(self, incoming: T, next_incoming_rx: Receiver<T>, is_closed: IsClosed) {
        // safety: the receiver must not be dropped before
        let _ = self.0.send((incoming, next_incoming_rx, is_closed));
    }
}

pub struct Receiver<T>(OneshotReceiver<(T, Self, IsClosed)>);

impl<T> Receiver<T> {
    async fn next(self) -> (T, Self, IsClosed) {
        // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
        let (incoming, next_incoming_rx, is_closed) = self.0.await.unwrap();
        (incoming, next_incoming_rx, is_closed)
    }
}
