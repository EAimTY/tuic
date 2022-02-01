use crate::connection::GetStreamRequest;
use quinn::{RecvStream, SendStream};
use std::{io::Error as IoError, net::SocketAddr};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
    },
    task::JoinHandle,
};
use tuic_protocol::{Address, Command, Error as TuicError, Reply, Request, Response};

pub struct Relay {
    token: u64,
    stream_req_tx: MpscSender<GetStreamRequest>,
    relay_req_rx: MpscReceiver<RelayRequest>,
}

impl Relay {
    pub fn init(
        token: u64,
        stream_req_tx: MpscSender<GetStreamRequest>,
    ) -> (JoinHandle<()>, MpscSender<RelayRequest>) {
        let (relay_req_tx, relay_req_rx) = mpsc::channel(32);

        let relay = {
            let mut relay = Self {
                token,
                stream_req_tx,
                relay_req_rx,
            };

            tokio::spawn(async move {
                while let Some(req) = relay.relay_req_rx.recv().await {
                    match req {
                        RelayRequest::Connect { addr, relay_res_tx } => {
                            relay.handle_connect(addr, relay_res_tx).await
                        }
                        RelayRequest::Associate { relay_res_tx } => {
                            relay.handle_associate(relay_res_tx).await
                        }
                    }
                }
            })
        };

        (relay, relay_req_tx)
    }

    async fn handle_connect(
        &self,
        addr: AddressRelay,
        relay_res_tx: OneshotSender<RelayResponseConnect>,
    ) {
        let (stream_req, stream_req_rx) = GetStreamRequest::new();
        let _ = self.stream_req_tx.send(stream_req).await;

        if let Some((mut send, mut recv)) = unsafe { stream_req_rx.await.unwrap_unchecked() } {
            async fn connect_handshake(
                tuic_req: Request,
                send: &mut SendStream,
                recv: &mut RecvStream,
            ) -> Result<(), RelayError> {
                tuic_req.write_to(send).await?;

                let tuic_res = Response::read_from(recv).await?;

                match tuic_res.reply {
                    Reply::Succeeded => Ok(()),
                    tuic_reply_err => Err(RelayError::Tuic(TuicError::from(tuic_reply_err))),
                }
            }

            let tuic_req = Request::new(Command::Connect, self.token, Address::from(addr));

            tokio::spawn(async move {
                let hs_res = connect_handshake(tuic_req, &mut send, &mut recv)
                    .await
                    .map(|()| (send, recv));

                let _ = relay_res_tx.send(hs_res);
            });
        } else {
            let _ = relay_res_tx.send(Err(RelayError::Connection));
        }
    }

    async fn handle_associate(&self, _relay_res_tx: OneshotSender<RelayResponseAssociate>) {
        todo!()
    }
}

pub enum RelayRequest {
    Connect {
        addr: AddressRelay,
        relay_res_tx: OneshotSender<RelayResponseConnect>,
    },
    Associate {
        relay_res_tx: OneshotSender<RelayResponseAssociate>,
    },
}

pub type RelayResponseConnect = Result<(SendStream, RecvStream), RelayError>;
pub type RelayResponseAssociate = ();

impl RelayRequest {
    pub fn new_connect(addr: AddressRelay) -> (Self, OneshotReceiver<RelayResponseConnect>) {
        let (relay_res_tx, relay_res_rx) = oneshot::channel();
        (Self::Connect { addr, relay_res_tx }, relay_res_rx)
    }

    pub fn new_associate() -> (Self, OneshotReceiver<RelayResponseAssociate>) {
        let (relay_res_tx, relay_res_rx) = oneshot::channel();
        (Self::Associate { relay_res_tx }, relay_res_rx)
    }
}

pub enum AddressRelay {
    SocketAddress(SocketAddr),
    HostnameAddress(String, u16),
}

impl From<AddressRelay> for Address {
    fn from(addr_relay: AddressRelay) -> Self {
        match addr_relay {
            AddressRelay::SocketAddress(addr) => Address::SocketAddress(addr),
            AddressRelay::HostnameAddress(hostname, port) => {
                Address::HostnameAddress(hostname, port)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum RelayError {
    #[error("Failed to open the connection to the server")]
    Connection,
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Tuic(#[from] TuicError),
}
