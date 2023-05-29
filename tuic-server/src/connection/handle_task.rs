use super::{UdpSession, ERROR_CODE};
use crate::{Connection, Error, UdpRelayMode};
use bytes::Bytes;
use std::{
    collections::hash_map::Entry,
    io::{Error as IoError, ErrorKind},
    net::SocketAddr,
};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{self, TcpStream},
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address;
use tuic_quinn::{Authenticate, Connect, Packet};

impl Connection {
    pub(super) async fn handle_authenticate(&self, auth: Authenticate) {
        log::info!(
            "[{addr}] [{uuid}] [authenticate] authenticated as {auth_uuid}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
            auth_uuid = auth.uuid(),
        );
    }

    pub(super) async fn handle_connect(&self, conn: Connect) {
        let target_addr = conn.addr().to_string();

        log::info!(
            "[{addr}] [{uuid}] [connect] {target_addr}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        let process = async {
            let mut stream = None;
            let mut last_err = None;

            match resolve_dns(conn.addr()).await {
                Ok(addrs) => {
                    for addr in addrs {
                        match TcpStream::connect(addr).await {
                            Ok(s) => {
                                stream = Some(s);
                                break;
                            }
                            Err(err) => last_err = Some(err),
                        }
                    }
                }
                Err(err) => last_err = Some(err),
            }

            if let Some(mut stream) = stream {
                let mut conn = conn.compat();
                let res = io::copy_bidirectional(&mut conn, &mut stream).await;
                let _ = conn.get_mut().reset(ERROR_CODE);
                let _ = stream.shutdown().await;
                res?;
                Ok::<_, Error>(())
            } else {
                let _ = conn.compat().shutdown().await;
                Err(last_err
                    .unwrap_or_else(|| IoError::new(ErrorKind::NotFound, "no address resolved")))?
            }
        };

        match process.await {
            Ok(()) => {}
            Err(err) => log::warn!(
                "[{addr}] [{uuid}] [connect] relaying connection to {target_addr} error: {err}",
                addr = self.inner.remote_address(),
                uuid = self.auth.get().unwrap(),
            ),
        }
    }

    pub(super) async fn handle_packet(&self, pkt: Packet, mode: UdpRelayMode) {
        let assoc_id = pkt.assoc_id();
        let pkt_id = pkt.pkt_id();
        let frag_id = pkt.frag_id();
        let frag_total = pkt.frag_total();

        log::info!(
            "[{addr}] [{uuid}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] {frag_id}/{frag_total}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        self.udp_relay_mode.store(Some(mode));

        let process = async {
            let Some((pkt, addr, assoc_id)) = pkt.accept().await? else {
                return Ok(());
            };

            let session = match self.udp_sessions.lock().entry(assoc_id) {
                Entry::Occupied(entry) => entry.get().clone(),
                Entry::Vacant(entry) => {
                    let session = UdpSession::new(
                        self.clone(),
                        assoc_id,
                        self.udp_relay_ipv6,
                        self.max_external_pkt_size,
                    )?;
                    entry.insert(session.clone());
                    session
                }
            };

            let Some(socket_addr) = resolve_dns(&addr).await?.next() else {
                return Err(Error::from(IoError::new(ErrorKind::NotFound, "no address resolved")));
            };

            session.send(pkt, socket_addr).await
        };

        match process.await {
            Ok(()) => {}
            Err(err) => log::warn!(
                "[{addr}] [{uuid}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] error handling fragment {frag_id}/{frag_total}: {err}",
                addr = self.inner.remote_address(),
                uuid = self.auth.get().unwrap(),
            ),
        }
    }

    pub(super) async fn handle_dissociate(&self, assoc_id: u16) {
        log::info!(
            "[{addr}] [{uuid}] [dissociate] [{assoc_id:#06x}]",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        if let Some(session) = self.udp_sessions.lock().remove(&assoc_id) {
            session.close();
        }
    }

    pub(super) async fn handle_heartbeat(&self) {
        log::info!(
            "[{addr}] [{uuid}] [heartbeat]",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );
    }

    pub(super) async fn send_packet(self, pkt: Bytes, addr: Address, assoc_id: u16) {
        let addr_display = addr.to_string();

        let res = match self.udp_relay_mode.load() {
            Some(UdpRelayMode::Native) => {
                log::info!(
                    "[{addr}] [packet-to-native] [{assoc_id}] [{target_addr}]",
                    addr = self.inner.remote_address(),
                    target_addr = addr_display,
                );
                self.model.packet_native(pkt, addr, assoc_id)
            }
            Some(UdpRelayMode::Quic) => {
                log::info!(
                    "[{addr}] [packet-to-quic] [{assoc_id}] [{target_addr}]",
                    addr = self.inner.remote_address(),
                    target_addr = addr_display,
                );
                self.model.packet_quic(pkt, addr, assoc_id).await
            }
            None => unreachable!(),
        };

        if let Err(err) = res {
            log::warn!(
                "[{addr}] [packet-to-native] [{assoc_id}] [{target_addr}] {err}",
                addr = self.inner.remote_address(),
                target_addr = addr_display,
            );
        }
    }
}

async fn resolve_dns(addr: &Address) -> Result<impl Iterator<Item = SocketAddr>, IoError> {
    match addr {
        Address::None => Err(IoError::new(ErrorKind::InvalidInput, "empty address")),
        Address::DomainAddress(domain, port) => Ok(net::lookup_host((domain.as_str(), *port))
            .await?
            .collect::<Vec<_>>()
            .into_iter()),
        Address::SocketAddress(addr) => Ok(vec![*addr].into_iter()),
    }
}
