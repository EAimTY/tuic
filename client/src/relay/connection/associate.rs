use super::{Address, Connection};
use anyhow::Result;
use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{Connection as QuinnConnection, Datagrams, IncomingUniStreams, RecvStream};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand};

impl Connection {
    pub fn handle_associate(
        &self,
        assoc_id: u32,
        mut pkt_send_rx: MpscReceiver<(Bytes, Address)>,
        pkt_receive_tx: MpscSender<(Bytes, Address)>,
    ) {
        let assoc_map = self.assoc_map.clone();
        assoc_map.insert(assoc_id, pkt_receive_tx);

        let conn = self.controller.clone();

        tokio::spawn(async move {
            while let Some((packet, addr)) = pkt_send_rx.recv().await {
                tokio::spawn(send_packet(conn.clone(), assoc_id, addr, packet));
            }

            assoc_map.remove(&assoc_id);
            dissociate(conn, assoc_id).await;
        });
    }

    pub async fn listen_incoming(
        mut uni_streams: IncomingUniStreams,
        _datagrams: Datagrams,
        assoc_map: Arc<AssociateMap>,
    ) {
        while let Some(res) = uni_streams.next().await {
            match res {
                Ok(stream) => {
                    tokio::spawn(receive_packet(stream, assoc_map.clone()));
                }
                Err(err) => {
                    eprintln!("{err}");
                    break;
                }
            }
        }
    }
}

async fn send_packet(conn: QuinnConnection, assoc_id: u32, addr: Address, packet: Bytes) {
    let res: Result<()> = try {
        let mut stream = conn.open_uni().await?;

        let addr = TuicAddress::from(addr);
        let cmd = TuicCommand::new_packet(assoc_id, packet.len() as u16, addr);

        cmd.write_to(&mut stream).await?;
        stream.write_chunk(packet).await?;
    };

    match res {
        Ok(()) => (),
        Err(err) => eprintln!("{err}"),
    }
}

async fn receive_packet(mut stream: RecvStream, assoc_map: Arc<AssociateMap>) {
    let res: Result<()> = try {
        let cmd = TuicCommand::read_from(&mut stream).await?;

        if let TuicCommand::Packet {
            assoc_id,
            len,
            addr,
        } = cmd
        {
            if let Some(pkt_receive_tx) = assoc_map.get(&assoc_id) {
                let mut buf = vec![0; len as usize];
                stream.read_exact(&mut buf).await?;

                let packet = Bytes::from(buf);
                let addr = Address::from(addr);

                let _ = pkt_receive_tx.send((packet, addr)).await;
            }
        }
    };

    match res {
        Ok(()) => (),
        Err(err) => eprintln!("{err}"),
    }
}

async fn dissociate(conn: QuinnConnection, assoc_id: u32) {
    let res: Result<()> = try {
        let mut stream = conn.open_uni().await?;
        let cmd = TuicCommand::new_dissociate(assoc_id);
        cmd.write_to(&mut stream).await?;
    };

    match res {
        Ok(()) => (),
        Err(err) => eprintln!("{err}"),
    }
}

pub struct AssociateMap(Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>);

impl AssociateMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    fn insert(&self, assoc_id: u32, pkt_receive_tx: MpscSender<(Bytes, Address)>) {
        self.0.lock().insert(assoc_id, pkt_receive_tx);
    }

    fn remove(&self, assoc_id: &u32) {
        self.0.lock().remove(&assoc_id);
    }

    fn get(&self, assoc_id: &u32) -> Option<MpscSender<(Bytes, Address)>> {
        self.0.lock().get(&assoc_id).cloned()
    }
}
