use super::{stream::BiStream, Address, Connection, UdpRelayMode};
use bytes::{Bytes, BytesMut};
use std::io::Result;
use tokio::{io::AsyncWriteExt, sync::oneshot::Sender as OneshotSender};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand};

impl Connection {
    pub async fn handle_connect(self, addr: Address, tx: OneshotSender<BiStream>) {
        async fn negotiate_connect(conn: Connection, addr: Address) -> Result<Option<BiStream>> {
            let cmd = TuicCommand::new_connect(TuicAddress::from(addr));

            let mut stream = conn.get_bi_stream().await?;
            cmd.write_to(&mut stream).await?;

            let resp = match TuicCommand::read_from(&mut stream).await {
                Ok(resp) => resp,
                Err(_err) => {
                    let _ = stream.shutdown().await;
                    todo!() // TODO: error handling
                }
            };

            if let TuicCommand::Response(true) = resp {
                Ok(Some(stream))
            } else {
                let _ = stream.shutdown().await;
                Ok(None)
            }
        }

        match negotiate_connect(self, addr).await {
            Ok(Some(stream)) => {
                let _ = tx.send(stream);
            }
            Ok(None) => {}
            Err(err) => log::error!("{err}"),
        }
    }

    pub async fn handle_packet_to(
        self,
        assoc_id: u32,
        pkt: Bytes,
        addr: Address,
        mode: UdpRelayMode<(), ()>,
    ) {
        async fn send_packet(
            conn: Connection,
            assoc_id: u32,
            pkt: Bytes,
            addr: Address,
            mode: UdpRelayMode<(), ()>,
        ) -> Result<()> {
            let cmd = TuicCommand::new_packet(assoc_id, pkt.len() as u16, TuicAddress::from(addr));

            match mode {
                UdpRelayMode::Native(()) => {
                    let mut buf = BytesMut::with_capacity(cmd.serialized_len());
                    cmd.write_to_buf(&mut buf);
                    buf.extend_from_slice(&pkt);
                    let pkt = buf.freeze();
                    conn.send_datagram(pkt)?;
                }
                UdpRelayMode::Quic(()) => {
                    let mut send = conn.get_send_stream().await?;
                    cmd.write_to(&mut send).await?;
                    let _ = send.shutdown().await;
                }
            }

            Ok(())
        }

        match send_packet(self, assoc_id, pkt, addr, mode).await {
            Ok(()) => {}
            Err(err) => log::error!("{err}"),
        }
    }

    pub async fn handle_packet_from(self, assoc_id: u32, pkt: Bytes, addr: Address) {
        if let Some(recv_pkt_tx) = self.udp_sessions().get(&assoc_id) {
            let _ = recv_pkt_tx.send((pkt, addr)).await;
        } else {
            log::error!("no recv_pkt_tx for assoc_id: {}", assoc_id);
        }
    }

    pub async fn handle_dissociate(self, assoc_id: u32) {
        async fn send_dissociate(conn: Connection, assoc_id: u32) -> Result<()> {
            let cmd = TuicCommand::new_dissociate(assoc_id);

            let mut send = conn.get_send_stream().await?;
            cmd.write_to(&mut send).await?;
            let _ = send.shutdown().await;

            Ok(())
        }

        match send_dissociate(self, assoc_id).await {
            Ok(()) => {}
            Err(err) => log::error!("{err}"),
        }
    }
}
