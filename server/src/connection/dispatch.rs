use super::{task, Connection, UdpPacketSource};
use bytes::Bytes;
use quinn::{RecvStream, SendStream, VarInt};
use std::io::Error as IoError;
use thiserror::Error;
use tuic_protocol::{Address, Command, UdpAssocPacket};

impl Connection {
    pub async fn process_uni_stream(&self, mut stream: RecvStream) -> Result<(), DispatchError> {
        let rmt_addr = self.controller.remote_address();
        let cmd = Command::read_from(&mut stream).await?;

        if let Command::Authenticate { digest } = cmd {
            if self.token.contains(&digest) {
                log::debug!("[{rmt_addr}] [authentication]");

                self.is_authenticated.set_authenticated();
                self.is_authenticated.wake();
                return Ok(());
            } else {
                let err = DispatchError::AuthenticationFailed;
                self.controller
                    .close(err.as_error_code(), err.to_string().as_bytes());
                self.is_authenticated.wake();
                return Err(err);
            }
        }

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Authenticate { .. } => unreachable!(),
                Command::Packet {
                    assoc_id,
                    len,
                    addr,
                } => {
                    if self.udp_packet_from.uni_stream() {
                        let dst_addr = addr.to_string();
                        log::debug!("[{rmt_addr}] [packet-from-quic] [{assoc_id}] [{dst_addr}]");

                        let res = task::packet_from_uni_stream(
                            stream,
                            self.udp_sessions.clone(),
                            assoc_id,
                            len,
                            addr,
                            rmt_addr,
                        )
                        .await;

                        match res {
                            Ok(()) => {}
                            Err(err) => log::warn!(
                                "[{rmt_addr}] [packet-from-quic] [{assoc_id}] [{dst_addr}] {err}"
                            ),
                        }

                        Ok(())
                    } else {
                        Err(DispatchError::BadCommand)
                    }
                }
                Command::Dissociate { assoc_id } => {
                    let res = task::dissociate(self.udp_sessions.clone(), assoc_id, rmt_addr).await;

                    match res {
                        Ok(()) => {}
                        Err(err) => log::warn!("[{rmt_addr}] [dissociate] {err}"),
                    }

                    Ok(())
                }
                Command::Heartbeat => {
                    log::debug!("[{rmt_addr}] [heartbeat]");
                    Ok(())
                }
                _ => Err(DispatchError::BadCommand),
            }
        } else {
            Err(DispatchError::AuthenticationTimeout)
        }
    }

    pub async fn process_bi_stream(
        &self,
        send: SendStream,
        mut recv: RecvStream,
    ) -> Result<(), DispatchError> {
        let cmd = Command::read_from(&mut recv).await?;
        let rmt_addr = self.controller.remote_address();

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Connect { addr } => {
                    let dst_addr = addr.to_string();
                    log::info!("[{rmt_addr}] [connect] [{dst_addr}]");

                    let res = task::connect(send, recv, addr).await;

                    match res {
                        Ok(()) => {}
                        Err(err) => log::warn!("[{rmt_addr}] [connect] [{dst_addr}] {err}"),
                    }

                    Ok(())
                }
                _ => Err(DispatchError::BadCommand),
            }
        } else {
            Err(DispatchError::AuthenticationTimeout)
        }
    }

    pub async fn process_datagram(&self, datagram: Bytes) -> Result<(), DispatchError> {
        let cmd = Command::read_from(&mut datagram.as_ref()).await?;
        let rmt_addr = self.controller.remote_address();
        let cmd_len = cmd.serialized_len();

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Packet { assoc_id, addr, .. } => {
                    if self.udp_packet_from.datagram() {
                        let dst_addr = addr.to_string();
                        log::debug!("[{rmt_addr}] [packet-from-native] [{assoc_id}] [{dst_addr}]");

                        let res = task::packet_from_datagram(
                            UdpAssocPacket::Regular {
                                addr,
                                pkt: datagram.slice(cmd_len..),
                            },
                            self.udp_sessions.clone(),
                            assoc_id,
                            rmt_addr,
                        )
                        .await;

                        match res {
                            Ok(()) => {}
                            Err(err) => {
                                log::warn!(
                                    "[{rmt_addr}] [packet-from-native] [{assoc_id}] [{dst_addr}] {err}"
                                )
                            }
                        }

                        Ok(())
                    } else {
                        Err(DispatchError::BadCommand)
                    }
                }
                Command::LongPacket {
                    assoc_id,
                    lp_id,
                    frag_id,
                    frag_cnt,
                    addr,
                    ..
                } => {
                    let dst_addr = addr
                        .as_ref()
                        .map(|addr| addr.to_string())
                        .unwrap_or_else(|| String::from("unspecified"));
                    log::debug!("[{rmt_addr}] [long-packet-from-native] [{assoc_id}] [{lp_id}] [{frag_id}/{frag_cnt}] [{dst_addr}]");
                    if self.udp_packet_from.datagram() {
                        let res = task::packet_from_datagram(
                            UdpAssocPacket::Long {
                                lp_id,
                                frag_id,
                                frag_cnt,
                                frag: datagram.slice(cmd_len..),
                                addr,
                            },
                            self.udp_sessions.clone(),
                            assoc_id,
                            rmt_addr,
                        )
                        .await;

                        if let Err(err) = res {
                            log::warn!("[{rmt_addr}] [long-packet-from-native] [{assoc_id}] [{lp_id}] [{frag_id}/{frag_cnt}] [{dst_addr}] {err}")
                        }

                        Ok(())
                    } else {
                        Err(DispatchError::BadCommand)
                    }
                }
                _ => Err(DispatchError::BadCommand),
            }
        } else {
            Err(DispatchError::AuthenticationTimeout)
        }
    }

    pub async fn process_received_udp_packet(
        &self,
        assoc_id: u32,
        pkt: Bytes,
        addr: Address,
    ) -> Result<(), DispatchError> {
        let rmt_addr = self.controller.remote_address();
        let dst_addr = addr.to_string();

        match self.udp_packet_from.check().unwrap() {
            UdpPacketSource::UniStream => {
                log::debug!("[{rmt_addr}] [packet-to-quic] [{assoc_id}] [{dst_addr}]");

                let res =
                    task::packet_to_uni_stream(self.controller.clone(), assoc_id, pkt, addr).await;

                match res {
                    Ok(()) => {}
                    Err(err) => {
                        log::warn!("[{rmt_addr}] [packet-to-quic] [{assoc_id}] [{dst_addr}] {err}")
                    }
                }
            }
            UdpPacketSource::Datagram => {
                let conn = self.controller.clone();
                let frag_size = conn.max_datagram_size().and_then(|mds| {
                    let frag_size = mds - Command::max_serialized_len();
                    if pkt.len() >= frag_size {
                        Some(frag_size)
                    } else {
                        None
                    }
                }); // Will max_datagram_size() ever return None when everything goes well?

                if let Some(frag_size) = frag_size {
                    if let Some(lp_id) = self.udp_sessions.next_lp_id(assoc_id) {
                        log::debug!(
                            "[{rmt_addr}] [long-packet-to-native] [{assoc_id}] [{lp_id}] [{dst_addr}]"
                        );
                        let res = task::long_packet_to_datagram(
                            conn, assoc_id, pkt, addr, frag_size, lp_id,
                        )
                        .await;
                        if let Err(err) = res {
                            log::warn!(
                                "[{rmt_addr}] [long-packet-to-native] [{assoc_id}] [{lp_id}] [{dst_addr}] {err}"
                            )
                        }
                    }
                } else {
                    log::debug!("[{rmt_addr}] [packet-to-native] [{assoc_id}] [{dst_addr}]");
                    let res =
                        task::packet_to_datagram(self.controller.clone(), assoc_id, pkt, addr)
                            .await;
                    if let Err(err) = res {
                        log::warn!(
                            "[{rmt_addr}] [packet-to-native] [{assoc_id}] [{dst_addr}] {err}"
                        )
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum DispatchError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("authentication timeout")]
    AuthenticationTimeout,
    #[error("bad command")]
    BadCommand,
}

impl DispatchError {
    const CODE_PROTOCOL: VarInt = VarInt::from_u32(0xfffffff0);
    const CODE_AUTHENTICATION_FAILED: VarInt = VarInt::from_u32(0xfffffff1);
    const CODE_AUTHENTICATION_TIMEOUT: VarInt = VarInt::from_u32(0xfffffff2);
    const CODE_BAD_COMMAND: VarInt = VarInt::from_u32(0xfffffff3);

    pub fn as_error_code(&self) -> VarInt {
        match self {
            Self::Io(_) => Self::CODE_PROTOCOL,
            Self::AuthenticationFailed => Self::CODE_AUTHENTICATION_FAILED,
            Self::AuthenticationTimeout => Self::CODE_AUTHENTICATION_TIMEOUT,
            Self::BadCommand => Self::CODE_BAD_COMMAND,
        }
    }
}
