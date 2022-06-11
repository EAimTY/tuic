use super::{connection::IsClosed, UdpRelayMode};
use futures_util::StreamExt;
use quinn::{ConnectionError, Datagrams, IncomingUniStreams};
use std::sync::Arc;
use tokio::sync::oneshot::{self, Receiver, Sender};

pub async fn listen_incoming(mut next: NextIncomingModeReceiver) {
    loop {
        let (incoming, is_closed) = match next {
            UdpRelayMode::Native(incoming_next) => {
                let (incoming, is_closed);
                (incoming, next, is_closed) = incoming_next.next().await;
                (incoming, is_closed)
            }
            UdpRelayMode::Quic(incoming_next) => {
                let (incoming, is_closed);
                (incoming, next, is_closed) = incoming_next.next().await;
                (incoming, is_closed)
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

type NextIncomingMode = UdpRelayMode<Datagrams, IncomingUniStreams>;

pub struct NextIncomingReceiver<T>(Receiver<(T, Self, IsClosed)>);
type NextIncomingSender<T> = Sender<(T, NextIncomingReceiver<T>, IsClosed)>;

pub type NextIncomingModeSender =
    UdpRelayMode<NextIncomingSender<Datagrams>, NextIncomingSender<IncomingUniStreams>>;
pub type NextIncomingModeReceiver =
    UdpRelayMode<NextIncomingReceiver<Datagrams>, NextIncomingReceiver<IncomingUniStreams>>;

impl NextIncomingReceiver<Datagrams> {
    pub fn new() -> (NextIncomingModeReceiver, NextIncomingModeSender, IsClosed) {
        let (tx, rx) = oneshot::channel();
        let is_closed = IsClosed::new();
        (
            NextIncomingModeReceiver::Native(Self(rx)),
            NextIncomingModeSender::Native(tx),
            is_closed,
        )
    }

    async fn next(self) -> (NextIncomingMode, NextIncomingModeReceiver, IsClosed) {
        // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
        let (incoming, next_rx, is_closed) = self.0.await.unwrap();
        (
            UdpRelayMode::Native(incoming),
            NextIncomingModeReceiver::Native(next_rx),
            is_closed,
        )
    }
}

impl NextIncomingReceiver<IncomingUniStreams> {
    pub fn new() -> (NextIncomingModeReceiver, NextIncomingModeSender, IsClosed) {
        let (tx, rx) = oneshot::channel();
        let is_closed = IsClosed::new();
        (
            NextIncomingModeReceiver::Quic(Self(rx)),
            NextIncomingModeSender::Quic(tx),
            is_closed,
        )
    }

    async fn next(self) -> (NextIncomingMode, NextIncomingModeReceiver, IsClosed) {
        // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
        let (incoming, next_rx, is_closed) = self.0.await.unwrap();
        (
            UdpRelayMode::Quic(incoming),
            NextIncomingModeReceiver::Quic(next_rx),
            is_closed,
        )
    }
}
