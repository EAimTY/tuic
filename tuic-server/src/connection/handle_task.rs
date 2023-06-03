use super::{Connection, UdpSession, ERROR_CODE};
use crate::{error::Error, utils::UdpRelayMode};
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
    pub async fn handle_authenticate(&self, auth: Authenticate) {
        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [authenticate] {auth_uuid}",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
            auth_uuid = auth.uuid(),
        );
    }

    pub async fn handle_connect(&self, conn: Connect) {
        let target_addr = conn.addr().to_string();

        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [connect] {target_addr}",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
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
                "[{id:#010x}] [{addr}] [{user}] [connect] {target_addr}: {err}",
                id = self.id(),
                addr = self.inner.remote_address(),
                user = self.auth,
            ),
        }
    }

    pub async fn handle_packet(&self, pkt: Packet, mode: UdpRelayMode) {
        let assoc_id = pkt.assoc_id();
        let pkt_id = pkt.pkt_id();
        let frag_id = pkt.frag_id();
        let frag_total = pkt.frag_total();

        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] fragment {frag_id}/{frag_total}",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
            frag_id = frag_id + 1,
        );

        self.udp_relay_mode.store(Some(mode));

        let (pkt, addr, assoc_id) = match pkt.accept().await {
            Ok(None) => return,
            Ok(Some(res)) => res,
            Err(err) => {
                log::warn!(
                    "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] fragment {frag_id}/{frag_total}: {err}",
                    id = self.id(),
                    addr = self.inner.remote_address(),
                    user = self.auth,
                    frag_id = frag_id + 1,
                );
                return;
            }
        };

        let process = async {
            log::info!(
                "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] to {src_addr}",
                id = self.id(),
                addr = self.inner.remote_address(),
                user = self.auth,
                src_addr = addr,
            );

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

        if let Err(err) = process.await {
            log::warn!(
                "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] to {src_addr}: {err}",
                id = self.id(),
                addr = self.inner.remote_address(),
                user = self.auth,
                src_addr = addr,
            );
        }
    }

    pub async fn handle_dissociate(&self, assoc_id: u16) {
        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [dissociate] [{assoc_id:#06x}]",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
        );

        if let Some(session) = self.udp_sessions.lock().remove(&assoc_id) {
            session.close();
        }
    }

    pub async fn handle_heartbeat(&self) {
        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [heartbeat]",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
        );
    }

    pub async fn relay_packet(self, pkt: Bytes, addr: Address, assoc_id: u16) {
        let addr_display = addr.to_string();

        log::info!(
            "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [to-{mode}] from {src_addr}",
            id = self.id(),
            addr = self.inner.remote_address(),
            user = self.auth,
            mode = self.udp_relay_mode.load().unwrap(),
            src_addr = addr_display,
        );

        let res = match self.udp_relay_mode.load().unwrap() {
            UdpRelayMode::Native => self.model.packet_native(pkt, addr, assoc_id),
            UdpRelayMode::Quic => self.model.packet_quic(pkt, addr, assoc_id).await,
        };

        if let Err(err) = res {
            log::warn!(
                "[{id:#010x}] [{addr}] [{user}] [packet] [{assoc_id:#06x}] [to-{mode}] from {src_addr}: {err}",
                id = self.id(),
                addr = self.inner.remote_address(),
                user = self.auth,
                mode = self.udp_relay_mode.load().unwrap(),
                src_addr = addr_display,
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
