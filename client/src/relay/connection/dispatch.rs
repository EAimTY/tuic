use super::{task, Connection};
use crate::{
    config::UdpMode,
    relay::{Address, RelayError, Request},
};
use bytes::Bytes;
use quinn::RecvStream;
use tuic_protocol::Command as TuicCommand;

impl Connection {
    pub async fn process_request(self, req: Request) -> Result<(), RelayError> {
        match req {
            Request::Connect { addr, tx } => {
                task::connect(self.controller, addr, tx).await?;
            }
            Request::Associate {
                assoc_id,
                mut pkt_send_rx,
                pkt_receive_tx,
            } => {
                self.udp_sessions.lock().insert(assoc_id, pkt_receive_tx);

                while let Some((pkt, addr)) = pkt_send_rx.recv().await {
                    let conn = self.controller.clone();

                    tokio::spawn(async move {
                        let res = match self.udp_mode {
                            UdpMode::Native => {
                                task::packet_to_datagram(conn, assoc_id, pkt, addr).await
                            }
                            UdpMode::Quic => {
                                task::packet_to_uni_stream(conn, assoc_id, pkt, addr).await
                            }
                        };

                        match res {
                            Ok(()) => (),
                            Err(err) => eprintln!("{err}"),
                        }
                    });
                }

                self.udp_sessions.lock().remove(&assoc_id);
                task::dissociate(self.controller, assoc_id).await?;
            }
        }

        Ok(())
    }

    pub async fn process_incoming_uni_stream(
        self,
        mut stream: RecvStream,
    ) -> Result<(), RelayError> {
        let cmd = TuicCommand::read_from(&mut stream).await?;

        match cmd {
            TuicCommand::Authenticate { .. } => Err(RelayError::BadCommand),
            TuicCommand::Connect { .. } => Err(RelayError::BadCommand),
            TuicCommand::Bind { .. } => Err(RelayError::BadCommand),
            TuicCommand::Packet {
                assoc_id,
                len,
                addr,
            } => {
                let mut buf = vec![0; len as usize];
                stream.read_exact(&mut buf).await.unwrap();

                let pkt = Bytes::from(buf);

                task::packet_from_server(pkt, self.udp_sessions, assoc_id, Address::from(addr))
                    .await
            }
            TuicCommand::Dissociate { .. } => Err(RelayError::BadCommand),
        }
    }

    pub async fn process_incoming_datagram(self, datagram: Bytes) -> Result<(), RelayError> {
        let cmd = TuicCommand::read_from(&mut datagram.as_ref()).await?;
        let cmd_len = cmd.serialized_len();

        match cmd {
            TuicCommand::Authenticate { .. } => Err(RelayError::BadCommand),
            TuicCommand::Connect { .. } => Err(RelayError::BadCommand),
            TuicCommand::Bind { .. } => Err(RelayError::BadCommand),
            TuicCommand::Packet { assoc_id, addr, .. } => {
                task::packet_from_server(
                    datagram.slice(cmd_len..),
                    self.udp_sessions,
                    assoc_id,
                    Address::from(addr),
                )
                .await
            }
            TuicCommand::Dissociate { .. } => Err(RelayError::BadCommand),
        }
    }
}
