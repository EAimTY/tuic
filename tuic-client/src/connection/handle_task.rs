use super::Connection;
use crate::{error::Error, socks5::UDP_SESSIONS as SOCKS5_UDP_SESSIONS, utils::UdpRelayMode};
use bytes::Bytes;
use quinn::ZeroRttAccepted;
use socks5_proto::Address as Socks5Address;
use std::time::Duration;
use tokio::time;
use tuic::Address;
use tuic_quinn::{Connect, Packet};

impl Connection {
    pub async fn authenticate(self, zero_rtt_accepted: Option<ZeroRttAccepted>) {
        if let Some(zero_rtt_accepted) = zero_rtt_accepted {
            log::debug!("[relay] [authenticate] waiting for connection to be fully established");
            zero_rtt_accepted.await;
        }

        log::debug!("[relay] [authenticate] sending authentication");

        match self
            .model
            .authenticate(self.uuid, self.password.clone())
            .await
        {
            Ok(()) => log::info!("[relay] [authenticate] {uuid}", uuid = self.uuid),
            Err(err) => log::warn!("[relay] [authenticate] authentication sending error: {err}"),
        }
    }

    pub async fn connect(&self, addr: Address) -> Result<Connect, Error> {
        let addr_display = addr.to_string();
        log::info!("[relay] [connect] {addr_display}");

        match self.model.connect(addr).await {
            Ok(conn) => Ok(conn),
            Err(err) => {
                log::warn!("[relay] [connect] failed initializing relay to {addr_display}: {err}");
                Err(Error::Model(err))
            }
        }
    }

    pub async fn packet(&self, pkt: Bytes, addr: Address, assoc_id: u16) -> Result<(), Error> {
        let addr_display = addr.to_string();

        match self.udp_relay_mode {
            UdpRelayMode::Native => {
                log::info!("[relay] [packet] [{assoc_id:#06x}] [to-native] to {addr_display}");
                match self.model.packet_native(pkt, addr, assoc_id) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::warn!("[relay] [packet] [{assoc_id:#06x}] [to-native] to {addr_display}: {err}");
                        Err(Error::Model(err))
                    }
                }
            }
            UdpRelayMode::Quic => {
                log::info!("[relay] [packet] [{assoc_id:#06x}] [to-quic] {addr_display}");
                match self.model.packet_quic(pkt, addr, assoc_id).await {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        log::warn!(
                            "[relay] [packet] [{assoc_id:#06x}] [to-quic] to {addr_display}: {err}"
                        );
                        Err(Error::Model(err))
                    }
                }
            }
        }
    }

    pub async fn dissociate(&self, assoc_id: u16) -> Result<(), Error> {
        log::info!("[relay] [dissociate] [{assoc_id:#06x}]");
        match self.model.dissociate(assoc_id).await {
            Ok(()) => Ok(()),
            Err(err) => {
                log::warn!("[relay] [dissociate] [{assoc_id:#06x}] {err}");
                Err(Error::Model(err))
            }
        }
    }

    pub async fn heartbeat(self, heartbeat: Duration) {
        loop {
            time::sleep(heartbeat).await;

            if self.is_closed() {
                break;
            }

            if self.model.task_connect_count() + self.model.task_associate_count() == 0 {
                continue;
            }

            match self.model.heartbeat().await {
                Ok(()) => log::debug!("[relay] [heartbeat]"),
                Err(err) => log::warn!("[relay] [heartbeat] {err}"),
            }
        }
    }

    pub async fn handle_packet(pkt: Packet) {
        let assoc_id = pkt.assoc_id();
        let pkt_id = pkt.pkt_id();

        let mode = if pkt.is_from_native() {
            "native"
        } else if pkt.is_from_quic() {
            "quic"
        } else {
            unreachable!()
        };

        log::info!(
            "[relay] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] fragment {frag_id}/{frag_total}",
            frag_id = pkt.frag_id() + 1,
            frag_total = pkt.frag_total(),
        );

        match pkt.accept().await {
            Ok(Some((pkt, addr, _))) => {
                log::info!("[relay] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] from {addr}");

                let addr = match addr {
                    Address::None => unreachable!(),
                    Address::DomainAddress(domain, port) => {
                        Socks5Address::DomainAddress(domain, port)
                    }
                    Address::SocketAddress(addr) => Socks5Address::SocketAddress(addr),
                };

                let session = SOCKS5_UDP_SESSIONS
                    .get()
                    .unwrap()
                    .lock()
                    .get(&assoc_id)
                    .cloned();

                if let Some(session) = session {
                    if let Err(err) = session.send(pkt, addr).await {
                        log::warn!(
                            "[relay] [packet] [{assoc_id:#06x}] [from-native] [{pkt_id:#06x}] failed sending packet to socks5 client: {err}",
                        );
                    }
                } else {
                    log::warn!("[relay] [packet] [{assoc_id:#06x}] [from-native] [{pkt_id:#06x}] unable to find socks5 associate session");
                }
            }
            Ok(None) => {}
            Err(err) => log::warn!("[relay] [packet] [{assoc_id:#06x}] [from-native] [{pkt_id:#06x}] packet receiving error: {err}"),
        }
    }
}
