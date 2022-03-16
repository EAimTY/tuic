use super::{handler, Connection};
use anyhow::Result;
use bytes::Bytes;
use quinn::{RecvStream, SendStream, VarInt};
use std::hint::unreachable_unchecked;
use tuic_protocol::{Address, Command};

impl Connection {
    pub async fn process_uni_stream(self, mut stream: RecvStream) {
        let cmd = match Command::read_from(&mut stream).await {
            Ok(cmd) => cmd,
            Err(err) => {
                eprintln!("{err}");
                self.controller.close(VarInt::MAX, b"Bad command");
                return;
            }
        };

        if let Command::Authenticate { digest } = cmd {
            if digest == self.expected_token_digest {
                self.is_authenticated.set_authenticated();
            } else {
                eprintln!("Authentication failed");
                self.controller.close(VarInt::MAX, b"Authentication failed");
            }

            self.authenticate_broadcast.wake();

            return;
        }

        if self.is_authenticated.await {
            match cmd {
                Command::Authenticate { .. } => unsafe { unreachable_unchecked() },
                Command::Connect { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Bind { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Packet {
                    assoc_id,
                    len,
                    addr,
                } => {
                    if self.udp_packet_from.uni_stream() {
                        tokio::spawn(handler::packet_from_uni_stream(
                            stream,
                            self.udp_sessions.clone(),
                            assoc_id,
                            len,
                            addr,
                        ));
                    } else {
                        self.controller.close(VarInt::MAX, b"Bad command")
                    }
                }
                Command::Dissociate { assoc_id } => {
                    tokio::spawn(handler::dissociate(self.udp_sessions.clone(), assoc_id));
                }
            }
        }
    }

    pub async fn process_bi_stream(self, send: SendStream, mut recv: RecvStream) {
        let cmd = match Command::read_from(&mut recv).await {
            Ok(cmd) => cmd,
            Err(err) => {
                eprintln!("{err}");
                self.controller.close(VarInt::MAX, b"Bad command");
                return;
            }
        };

        if self.is_authenticated.await {
            match cmd {
                Command::Authenticate { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Connect { addr } => {
                    tokio::spawn(handler::connect(send, recv, addr));
                }
                Command::Bind { addr } => {
                    tokio::spawn(handler::bind(send, recv, addr));
                }
                Command::Packet { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Dissociate { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
            }
        }
    }

    pub async fn process_datagram(self, datagram: Bytes) {
        let cmd = match Command::read_from(&mut datagram.as_ref()).await {
            Ok(cmd) => cmd,
            Err(err) => {
                eprintln!("{err}");
                self.controller.close(VarInt::MAX, b"Bad command");
                return;
            }
        };
        let cmd_len = cmd.serialized_len();

        if self.is_authenticated.await {
            match cmd {
                Command::Authenticate { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Connect { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Bind { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
                Command::Packet {
                    assoc_id,
                    len,
                    addr,
                } => {
                    if self.udp_packet_from.datagram() {
                        tokio::spawn(handler::packet_from_datagram(
                            datagram.slice(cmd_len..),
                            self.udp_sessions.clone(),
                            assoc_id,
                            len,
                            addr,
                        ));
                    } else {
                        self.controller.close(VarInt::MAX, b"Bad command")
                    }
                }
                Command::Dissociate { .. } => self.controller.close(VarInt::MAX, b"Bad command"),
            }
        }
    }

    pub async fn process_received_udp_packet(self, assoc_id: u32, packet: Vec<u8>, addr: Address) {
        let res: Result<()> = try {
            let mut stream = self.controller.open_uni().await?;

            let cmd = Command::new_packet(assoc_id, packet.len() as u16, addr);
            cmd.write_to(&mut stream).await?;

            stream.write_all(&packet).await?;
        };

        match res {
            Ok(()) => {}
            Err(err) => eprintln!("{err}"),
        }
    }
}
