use super::{stream::BiStream, Address, Connection, UdpRelayMode};
use bytes::{Bytes, BytesMut};
use std::io::Result;
use tokio::{io::AsyncWriteExt, sync::oneshot::Sender as OneshotSender};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, UdpAssocPacket};

impl Connection {
    pub async fn handle_connect(self, addr: Address, tx: OneshotSender<BiStream>) {
        async fn negotiate_connect(conn: Connection, addr: Address) -> Result<Option<BiStream>> {
            let cmd = TuicCommand::new_connect(TuicAddress::from(addr));

            let mut stream = conn.get_bi_stream().await?;
            cmd.write_to(&mut stream).await?;

            let resp = match TuicCommand::read_from(&mut stream).await {
                Ok(resp) => resp,
                Err(err) => {
                    stream.finish().await?;
                    return Err(err);
                }
            };

            if let TuicCommand::Response(true) = resp {
                Ok(Some(stream))
            } else {
                stream.finish().await?;
                Ok(None)
            }
        }

        let display_addr = format!("{addr}");

        match negotiate_connect(self, addr).await {
            Ok(Some(stream)) => {
                log::debug!("[relay] [task] [connect] [{display_addr}] [success]");
                let _ = tx.send(stream);
            }
            Ok(None) => log::debug!("[relay] [task] [connect] [{display_addr}] [fail]"),
            Err(err) => log::warn!("[relay] [task] [connect] [{display_addr}] {err}"),
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
            match mode {
                UdpRelayMode::Native(()) => {
                    let frag_size = conn.fragment_size();
                    if pkt.len() > frag_size {
                        let frags = pkt.chunks(frag_size);
                        let frag_cnt = frags.len();
                        let addr = TuicAddress::from(addr);
                        if let Some(session) = conn.udp_sessions().get(&assoc_id) {
                            let lp_id = session.next_lp_id();
                            for (i, frag) in frags.enumerate() {
                                let cmd = TuicCommand::new_long_packet(
                                    assoc_id,
                                    frag.len() as u16,
                                    lp_id,
                                    i as u8,
                                    frag_cnt as u8,
                                    if i == 0 { Some(addr.clone()) } else { None },
                                );
                                log::debug!(
                                    "[relay] [task] [associate] [{assoc_id}] [send] [{lp_id}] [{i}/{frag_cnt}]"
                                );
                                let mut buf = BytesMut::with_capacity(cmd.serialized_len());
                                cmd.write_to_buf(&mut buf);
                                buf.extend_from_slice(frag);
                                conn.send_datagram(buf.freeze())?;
                            }
                        }
                    } else {
                        let cmd = TuicCommand::new_packet(
                            assoc_id,
                            pkt.len() as u16,
                            TuicAddress::from(addr),
                        );
                        let mut buf = BytesMut::with_capacity(cmd.serialized_len());
                        cmd.write_to_buf(&mut buf);
                        buf.extend_from_slice(&pkt);
                        conn.send_datagram(buf.freeze())?;
                    }
                }
                UdpRelayMode::Quic(()) => {
                    let cmd = TuicCommand::new_packet(
                        assoc_id,
                        pkt.len() as u16,
                        TuicAddress::from(addr),
                    );
                    let mut send = conn.get_send_stream().await?;
                    cmd.write_to(&mut send).await?;
                    send.write_all(&pkt).await?;
                    send.finish().await?;
                }
            }

            Ok(())
        }

        self.update_max_udp_relay_packet_size();
        let display_addr = format!("{addr}");

        match send_packet(self, assoc_id, pkt, addr, mode).await {
            Ok(()) => log::debug!(
                "[relay] [task] [associate] [{assoc_id}] [send] [{display_addr}] [success]"
            ),
            Err(err) => {
                log::warn!("[relay] [task] [associate] [{assoc_id}] [send] [{display_addr}] {err}")
            }
        }
    }

    pub async fn handle_packet_from(self, assoc_id: u32, pkt: UdpAssocPacket) {
        self.update_max_udp_relay_packet_size();
        let display_addr = match &pkt {
            UdpAssocPacket::Regular { addr, .. } => format!("{addr}"),
            UdpAssocPacket::Long {
                addr: Some(addr), ..
            } => format!("{addr}"),
            _ => String::from("unspecified"),
        };

        if let Some(session) = self.udp_sessions().get(&assoc_id) {
            match pkt {
                UdpAssocPacket::Regular { addr, pkt } => {
                    log::debug!(
                        "[relay] [task] [associate] [{assoc_id}] [recv] [{display_addr}] [success]"
                    );
                    let _ = session.sender().send((pkt, addr.into())).await;
                }
                UdpAssocPacket::Long {
                    lp_id,
                    frag_id,
                    frag_cnt,
                    frag,
                    addr,
                } => {
                    let result = {
                        let mut lbps = session.lpbs().lock();
                        lbps.on_frag(lp_id, frag_id, frag_cnt, addr, frag)
                    };
                    if let Some((pkt, addr)) = result {
                        log::debug!(
                            "[relay] [task] [associate] [{assoc_id}] [recv] [{display_addr}] [long {frag_cnt}] [success]"
                        );
                        let _ = session.sender().send((pkt, addr.into())).await;
                    } else {
                        log::debug!(
                            "[relay] [task] [associate] [{assoc_id}] [recv] [{display_addr}] [long {frag_id}/{frag_cnt}]"
                        );
                    }
                }
            }
        } else {
            log::warn!("[relay] [task] [associate] [{assoc_id}] [recv] [{display_addr}] No corresponding UDP relay session found");
        }
    }

    pub async fn handle_dissociate(self, assoc_id: u32) {
        async fn send_dissociate(conn: Connection, assoc_id: u32) -> Result<()> {
            let cmd = TuicCommand::new_dissociate(assoc_id);

            let mut send = conn.get_send_stream().await?;
            cmd.write_to(&mut send).await?;
            send.finish().await?;

            Ok(())
        }

        match send_dissociate(self, assoc_id).await {
            Ok(()) => log::debug!("[relay] [task] [dissociate] [{assoc_id}] [success]"),
            Err(err) => log::warn!("relay] [task] [dissociate] [{assoc_id}] {err}"),
        }
    }
}
