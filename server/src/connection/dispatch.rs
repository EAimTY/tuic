use super::{task, Connection, UdpPacketSource};
use bytes::Bytes;
use quinn::{RecvStream, SendStream, VarInt};
use std::hint::unreachable_unchecked;
use thiserror::Error;
use tuic_protocol::{Address, Command, Error as ProtocolError};

impl Connection {
    pub async fn process_uni_stream(&self, mut stream: RecvStream) -> Result<(), DispatchError> {
        let cmd = Command::read_from(&mut stream).await?;

        if let Command::Authenticate { digest } = cmd {
            if digest == self.expected_token_digest {
                self.is_authenticated.set_authenticated();
                self.authenticate_broadcast.wake();
                return Ok(());
            } else {
                self.authenticate_broadcast.wake();
                return Err(DispatchError::AuthenticationFailed);
            }
        }

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Authenticate { .. } => unsafe { unreachable_unchecked() },
                Command::Connect { .. } => Err(DispatchError::BadCommand),
                Command::Bind { .. } => Err(DispatchError::BadCommand),
                Command::Packet {
                    assoc_id,
                    len,
                    addr,
                } => {
                    if self.udp_packet_from.uni_stream() {
                        let res = task::packet_from_uni_stream(
                            stream,
                            self.udp_sessions.clone(),
                            assoc_id,
                            len,
                            addr,
                        )
                        .await;

                        match res {
                            Ok(()) => {}
                            Err(err) => eprintln!("{err}"),
                        }

                        Ok(())
                    } else {
                        Err(DispatchError::BadCommand)
                    }
                }
                Command::Dissociate { assoc_id } => {
                    let res = task::dissociate(self.udp_sessions.clone(), assoc_id).await;

                    match res {
                        Ok(()) => {}
                        Err(err) => eprintln!("{err}"),
                    }

                    Ok(())
                }
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

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Authenticate { .. } => Err(DispatchError::BadCommand),
                Command::Connect { addr } => {
                    let res = task::connect(send, recv, addr).await;

                    match res {
                        Ok(()) => {}
                        Err(err) => eprintln!("{err}"),
                    }

                    Ok(())
                }
                Command::Bind { addr } => {
                    let res = task::bind(send, recv, addr).await;

                    match res {
                        Ok(()) => {}
                        Err(err) => eprintln!("{err}"),
                    }

                    Ok(())
                }
                Command::Packet { .. } => Err(DispatchError::BadCommand),
                Command::Dissociate { .. } => Err(DispatchError::BadCommand),
            }
        } else {
            Err(DispatchError::AuthenticationTimeout)
        }
    }

    pub async fn process_datagram(&self, datagram: Bytes) -> Result<(), DispatchError> {
        let cmd = Command::read_from(&mut datagram.as_ref()).await?;
        let cmd_len = cmd.serialized_len();

        if self.is_authenticated.clone().await {
            match cmd {
                Command::Authenticate { .. } => Err(DispatchError::BadCommand),
                Command::Connect { .. } => Err(DispatchError::BadCommand),
                Command::Bind { .. } => Err(DispatchError::BadCommand),
                Command::Packet { assoc_id, addr, .. } => {
                    if self.udp_packet_from.datagram() {
                        let res = task::packet_from_datagram(
                            datagram.slice(cmd_len..),
                            self.udp_sessions.clone(),
                            assoc_id,
                            addr,
                        )
                        .await;

                        match res {
                            Ok(()) => {}
                            Err(err) => eprintln!("{err}"),
                        }

                        Ok(())
                    } else {
                        Err(DispatchError::BadCommand)
                    }
                }
                Command::Dissociate { .. } => Err(DispatchError::BadCommand),
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
        match unsafe { self.udp_packet_from.check().unwrap_unchecked() } {
            UdpPacketSource::UniStream => {
                let res =
                    task::packet_to_uni_stream(self.controller.clone(), assoc_id, pkt, addr).await;

                match res {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            }
            UdpPacketSource::Datagram => {
                let res =
                    task::packet_to_datagram(self.controller.clone(), assoc_id, pkt, addr).await;

                match res {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            }
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum DispatchError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Authentication timeout")]
    AuthenticationTimeout,
    #[error("Bad command")]
    BadCommand,
}

impl DispatchError {
    const CODE_PROTOCOL: VarInt = VarInt::from_u32(0xfffffff0);
    const CODE_AUTHENTICATION_FAILED: VarInt = VarInt::from_u32(0xfffffff1);
    const CODE_AUTHENTICATION_TIMEOUT: VarInt = VarInt::from_u32(0xfffffff2);
    const CODE_BAD_COMMAND: VarInt = VarInt::from_u32(0xfffffff3);

    pub fn as_error_code(&self) -> VarInt {
        match self {
            Self::Protocol(_) => Self::CODE_PROTOCOL,
            Self::AuthenticationFailed => Self::CODE_AUTHENTICATION_FAILED,
            Self::AuthenticationTimeout => Self::CODE_AUTHENTICATION_TIMEOUT,
            Self::BadCommand => Self::CODE_BAD_COMMAND,
        }
    }
}
