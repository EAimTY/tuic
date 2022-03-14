use anyhow::Result;
use quinn::{Connecting, Connection as QuinnConnection, NewConnection};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use tokio::sync::mpsc::Receiver as MpscReceiver;
use tuic_protocol::{Address, Command};

pub use self::associate::AssociateMap;

mod associate;
mod bi_stream;
mod datagram;
mod uni_stream;

pub struct Connection {
    controller: QuinnConnection,
    assoc_map: Arc<AssociateMap>,
    is_authenticated: Arc<AtomicBool>,
    create_time: Instant,
}

impl Connection {
    pub async fn handle(conn: Connecting, expected_token_digest: [u8; 32]) {
        match conn.await {
            Ok(NewConnection {
                connection,
                uni_streams,
                bi_streams,
                datagrams,
                ..
            }) => {
                let (assoc_map, packet_receive_rx) = AssociateMap::new();

                let conn = Self {
                    controller: connection,
                    assoc_map: Arc::new(assoc_map),
                    is_authenticated: Arc::new(AtomicBool::new(false)),
                    create_time: Instant::now(),
                };

                tokio::join!(
                    conn.listen_uni_streams(uni_streams, expected_token_digest),
                    conn.listen_bi_streams(bi_streams),
                    conn.listen_datagrams(datagrams),
                    listen_packet_receive(conn.controller.clone(), packet_receive_rx)
                );
            }
            Err(err) => eprintln!("{err}"),
        }
    }
}

async fn listen_packet_receive(
    conn: QuinnConnection,
    mut packet_receive_rx: MpscReceiver<(u32, Vec<u8>, Address)>,
) {
    while let Some((assoc_id, packet, addr)) = packet_receive_rx.recv().await {
        tokio::spawn(send_received_packet(conn.clone(), assoc_id, packet, addr));
    }
}

async fn send_received_packet(
    conn: QuinnConnection,
    assoc_id: u32,
    packet: Vec<u8>,
    addr: Address,
) {
    let res: Result<()> = try {
        let mut stream = conn.open_uni().await?;
        let cmd = Command::new_packet(assoc_id, packet.len() as u16, addr);
        cmd.write_to(&mut stream).await?;
        stream.write_all(&packet).await?;
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }
}
