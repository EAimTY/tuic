use anyhow::{bail, Result};
use bytes::Bytes;
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
    ops::Deref,
    sync::Arc,
};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver, Sender},
};
use tuic_protocol::Address;

#[derive(Clone)]
pub struct UdpPacketFrom(Arc<AtomicCell<Option<UdpPacketSource>>>);

impl UdpPacketFrom {
    pub fn new() -> Self {
        Self(Arc::new(AtomicCell::new(None)))
    }

    pub fn check(&self) -> Option<UdpPacketSource> {
        self.0.load()
    }

    pub fn uni_stream(&self) -> bool {
        self.0
            .compare_exchange(None, Some(UdpPacketSource::UniStream))
            .map_or_else(|from| from == Some(UdpPacketSource::UniStream), |_| true)
    }

    pub fn datagram(&self) -> bool {
        self.0
            .compare_exchange(None, Some(UdpPacketSource::Datagram))
            .map_or_else(|from| from == Some(UdpPacketSource::Datagram), |_| true)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum UdpPacketSource {
    UniStream,
    Datagram,
}

pub type SendPacketSender = Sender<(Bytes, Address)>;
pub type SendPacketReceiver = Receiver<(Bytes, Address)>;
pub type RecvPacketSender = Sender<(u32, Bytes, Address)>;
pub type RecvPacketReceiver = Receiver<(u32, Bytes, Address)>;

pub struct UdpSessionMap {
    map: Mutex<HashMap<u32, UdpSession>>,
    recv_pkt_tx_for_clone: RecvPacketSender,
}

impl UdpSessionMap {
    pub fn new() -> (Self, RecvPacketReceiver) {
        let (recv_pkt_tx, recv_pkt_rx) = mpsc::channel(1);

        (
            Self {
                map: Mutex::new(HashMap::new()),
                recv_pkt_tx_for_clone: recv_pkt_tx,
            },
            recv_pkt_rx,
        )
    }

    pub async fn send(&self, assoc_id: u32, pkt: Bytes, addr: Address) {
        let mut map = self.map.lock();

        match map.entry(assoc_id) {
            Entry::Occupied(entry) => {
                let _ = entry.get().send((pkt, addr)).await;
            }
            Entry::Vacant(entry) => {
                match UdpSession::new(assoc_id, self.recv_pkt_tx_for_clone.clone()).await {
                    Ok(assoc) => {
                        let _ = entry.insert(assoc).send((pkt, addr)).await;
                    }
                    Err(err) => eprintln!("{err}"),
                }
            }
        }
    }

    pub fn dissociate(&self, assoc_id: u32) {
        self.map.lock().remove(&assoc_id);
    }
}

struct UdpSession(SendPacketSender);

impl UdpSession {
    async fn new(assoc_id: u32, recv_pkt_tx: RecvPacketSender) -> Result<Self> {
        let socket = Arc::new(UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?);
        let (send_pkt_tx, send_pkt_rx) = mpsc::channel(1);

        tokio::spawn(async move {
            match tokio::try_join!(
                Self::listen_send(socket.clone(), send_pkt_rx),
                Self::listen_receive(socket, assoc_id, recv_pkt_tx)
            ) {
                Ok(((), ())) => {}
                Err(err) => eprintln!("{err}"),
            }
        });

        Ok(Self(send_pkt_tx))
    }

    async fn listen_send(
        socket: Arc<UdpSocket>,
        mut send_pkt_rx: SendPacketReceiver,
    ) -> Result<()> {
        while let Some((pkt, addr)) = send_pkt_rx.recv().await {
            let socket = socket.clone();

            tokio::spawn(async move {
                let res = match addr {
                    Address::HostnameAddress(hostname, port) => {
                        socket.send_to(&pkt, (hostname, port)).await
                    }
                    Address::SocketAddress(addr) => socket.send_to(&pkt, addr).await,
                };

                match res {
                    Ok(_) => {}
                    Err(err) => eprintln!("{err}"),
                }
            });
        }

        bail!("Dissociated");
    }

    async fn listen_receive(
        socket: Arc<UdpSocket>,
        assoc_id: u32,
        recv_pkt_tx: RecvPacketSender,
    ) -> Result<()> {
        loop {
            let mut buf = vec![0; 1536];

            match socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    buf.truncate(len);
                    let pkt = Bytes::from(buf);

                    let _ = recv_pkt_tx
                        .send((assoc_id, pkt, Address::SocketAddress(addr)))
                        .await;
                }
                Err(err) => eprintln!("{err}"),
            }
        }
    }
}

impl Deref for UdpSession {
    type Target = SendPacketSender;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
