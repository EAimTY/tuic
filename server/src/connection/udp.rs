use bytes::Bytes;
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;

use std::{
    collections::HashMap,
    io::Result,
    net::{Ipv6Addr, SocketAddr},
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
    max_pkt_size: usize,
}

impl UdpSessionMap {
    pub fn new(max_pkt_size: usize) -> (Self, RecvPacketReceiver) {
        let (recv_pkt_tx, recv_pkt_rx) = mpsc::channel(1);

        (
            Self {
                map: Mutex::new(HashMap::new()),
                recv_pkt_tx_for_clone: recv_pkt_tx,
                max_pkt_size,
            },
            recv_pkt_rx,
        )
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn send(
        &self,
        assoc_id: u32,
        pkt: Bytes,
        addr: Address,
        src_addr: SocketAddr,
    ) -> Result<()> {
        let map = self.map.lock();

        let send_pkt_tx = if let Some(session) = map.get(&assoc_id) {
            let send_pkt_tx = session.0.clone();
            drop(map);
            send_pkt_tx
        } else {
            log::info!("[{src_addr}] [associate] [{assoc_id}]");
            drop(map);

            let assoc = UdpSession::new(
                assoc_id,
                self.recv_pkt_tx_for_clone.clone(),
                src_addr,
                self.max_pkt_size,
            )
            .await?;

            let send_pkt_tx = assoc.0.clone();

            let mut map = self.map.lock();
            map.insert(assoc_id, assoc);

            send_pkt_tx
        };

        let _ = send_pkt_tx.send((pkt, addr)).await;

        Ok(())
    }

    pub fn dissociate(&self, assoc_id: u32, src_addr: SocketAddr) {
        log::info!("[{src_addr}] [dissociate] [{assoc_id}]");
        self.map.lock().remove(&assoc_id);
    }
}

struct UdpSession(SendPacketSender);

impl UdpSession {
    async fn new(
        assoc_id: u32,
        recv_pkt_tx: RecvPacketSender,
        src_addr: SocketAddr,
        max_pkt_size: usize,
    ) -> Result<Self> {
        let socket = Arc::new(UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).await?);
        let (send_pkt_tx, send_pkt_rx) = mpsc::channel(1);

        tokio::spawn(async move {
            match tokio::select! {
                res = Self::listen_send_packet(socket.clone(), send_pkt_rx) => res,
                res = Self::listen_receive_packet(socket, assoc_id, recv_pkt_tx, max_pkt_size) => res,
            } {
                Ok(()) => (),
                Err(err) => log::warn!("[{src_addr}] [udp-session] [{assoc_id}] {err}"),
            }
        });

        Ok(Self(send_pkt_tx))
    }

    async fn listen_send_packet(
        socket: Arc<UdpSocket>,
        mut send_pkt_rx: SendPacketReceiver,
    ) -> Result<()> {
        while let Some((pkt, addr)) = send_pkt_rx.recv().await {
            match addr {
                Address::DomainAddress(hostname, port) => {
                    socket.send_to(&pkt, (hostname, port)).await?;
                }
                Address::SocketAddress(addr) => {
                    socket.send_to(&pkt, addr).await?;
                }
            }
        }

        Ok(())
    }

    async fn listen_receive_packet(
        socket: Arc<UdpSocket>,
        assoc_id: u32,
        recv_pkt_tx: RecvPacketSender,
        max_pkt_size: usize,
    ) -> Result<()> {
        loop {
            let mut buf = vec![0; max_pkt_size];
            let (len, addr) = socket.recv_from(&mut buf).await?;
            buf.truncate(len);

            let pkt = Bytes::from(buf);
            let _ = recv_pkt_tx
                .send((assoc_id, pkt, Address::SocketAddress(addr)))
                .await;
        }
    }
}
