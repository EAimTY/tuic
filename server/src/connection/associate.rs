use anyhow::{bail, Result};
use parking_lot::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
    sync::Arc,
};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
};
use tuic_protocol::Address;

pub struct AssociateMap {
    map: Mutex<HashMap<u32, MpscSender<(Vec<u8>, Address)>>>,
    packet_receive_tx: MpscSender<(u32, Vec<u8>, Address)>,
}

impl AssociateMap {
    pub fn new() -> (Self, MpscReceiver<(u32, Vec<u8>, Address)>) {
        let (packet_receive_tx, packet_receive_rx) = mpsc::channel(1);

        (
            Self {
                map: Mutex::new(HashMap::new()),
                packet_receive_tx,
            },
            packet_receive_rx,
        )
    }

    pub async fn send(&self, assoc_id: u32, packet: Vec<u8>, addr: Address) {
        let mut map = self.map.lock();

        match map.entry(assoc_id) {
            Entry::Occupied(entry) => {
                let _ = entry.get().send((packet, addr)).await;
            }
            Entry::Vacant(entry) => {
                match new_associate(assoc_id, self.packet_receive_tx.clone()).await {
                    Ok(assoc) => {
                        let _ = entry.insert(assoc).send((packet, addr)).await;
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

async fn new_associate(
    assoc_id: u32,
    packet_receive_tx: MpscSender<(u32, Vec<u8>, Address)>,
) -> Result<MpscSender<(Vec<u8>, Address)>> {
    let socket = Arc::new(UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?);
    let (packet_send_tx, packet_send_rx) = mpsc::channel(1);

    tokio::spawn(async move {
        match tokio::try_join!(
            listen_send(socket.clone(), packet_send_rx),
            listen_receive(socket, assoc_id, packet_receive_tx)
        ) {
            Ok(((), ())) => {}
            Err(err) => eprintln!("{err}"),
        }
    });

    Ok(packet_send_tx)
}

async fn listen_send(
    socket: Arc<UdpSocket>,
    mut packet_send_rx: MpscReceiver<(Vec<u8>, Address)>,
) -> Result<()> {
    while let Some((packet, addr)) = packet_send_rx.recv().await {
        tokio::spawn(send_packet(socket.clone(), packet, addr));
    }

    bail!("Dissociated");
}

async fn send_packet(socket: Arc<UdpSocket>, packet: Vec<u8>, addr: Address) {
    let res = match addr {
        Address::HostnameAddress(hostname, port) => socket.send_to(&packet, (hostname, port)).await,
        Address::SocketAddress(addr) => socket.send_to(&packet, addr).await,
    };

    match res {
        Ok(_) => {}
        Err(err) => eprintln!("{err}"),
    }
}

async fn listen_receive(
    socket: Arc<UdpSocket>,
    assoc_id: u32,
    packet_receive_tx: MpscSender<(u32, Vec<u8>, Address)>,
) -> Result<()> {
    loop {
        let mut buf = vec![0; 1536];
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                buf.truncate(len);

                let _ = packet_receive_tx
                    .send((assoc_id, buf, Address::SocketAddress(addr)))
                    .await;
            }
            Err(err) => eprintln!("{err}"),
        }
    }
}
