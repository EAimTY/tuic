use super::{connection::IsClosed, UdpRelayMode};
use futures_util::StreamExt;
use quinn::{ConnectionError, Datagrams, IncomingUniStreams};
use std::sync::Arc;
use tokio::sync::oneshot::{self, Receiver, Sender};

pub async fn listen_incoming(
    udp_relay_mode: UdpRelayMode,
    mut dg_next: NextDatagrams,
    mut uni_next: NextIncomingUniStreams,
) {
    match udp_relay_mode {
        UdpRelayMode::Native => loop {
            let (mut datagrams, even_next, is_closed) = dg_next.next().await;
            dg_next = even_next;

            let err = loop {
                let pkt = if let Some(pkt) = datagrams.next().await {
                    match pkt {
                        Ok(pkt) => pkt,
                        Err(err) => break err,
                    }
                } else {
                    break ConnectionError::LocallyClosed;
                };

                // TODO: process datagram
                tokio::spawn(async move {});
            };

            match err {
                ConnectionError::LocallyClosed | ConnectionError::TimedOut => {}
                err => log::error!("[relay] [connection] {err}"),
            }

            is_closed.set_closed();
        },
        UdpRelayMode::Quic => loop {
            let (mut uni_streams, even_next, is_closed) = uni_next.next().await;
            uni_next = even_next;

            let err = loop {
                let stream = if let Some(stream) = uni_streams.next().await {
                    match stream {
                        Ok(stream) => stream,
                        Err(err) => break err,
                    }
                } else {
                    break ConnectionError::LocallyClosed;
                };

                // TODO: process stream
                tokio::spawn(async move {});
            };

            match err {
                ConnectionError::LocallyClosed | ConnectionError::TimedOut => {}
                err => log::error!("[relay] [connection] {err}"),
            }

            is_closed.set_closed();
        },
    }
}

pub struct NextDatagrams(Receiver<(Datagrams, NextDatagrams, IsClosed)>);
pub type NextDatagramsSender = Sender<(Datagrams, NextDatagrams, IsClosed)>;

impl NextDatagrams {
    pub fn new() -> (Self, NextDatagramsSender, IsClosed) {
        let (tx, rx) = oneshot::channel();
        let is_closed = IsClosed::new();
        (Self(rx), tx, is_closed)
    }

    async fn next(self) -> (Datagrams, Self, IsClosed) {
        self.0.await.unwrap() // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
    }
}

pub struct NextIncomingUniStreams(Receiver<(IncomingUniStreams, NextIncomingUniStreams, IsClosed)>);
pub type NextIncomingUniStreamsSender =
    Sender<(IncomingUniStreams, NextIncomingUniStreams, IsClosed)>;

impl NextIncomingUniStreams {
    pub fn new() -> (Self, NextIncomingUniStreamsSender, IsClosed) {
        let (tx, rx) = oneshot::channel();
        let is_closed = IsClosed::new();
        (Self(rx), tx, is_closed)
    }

    async fn next(self) -> (IncomingUniStreams, Self, IsClosed) {
        self.0.await.unwrap() // safety: the current task that waiting new incoming will be cancelled if the sender's scope is dropped
    }
}
