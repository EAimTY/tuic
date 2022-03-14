use super::Address;
use anyhow::Result;
use bytes::Bytes;
use quinn::{
    Connecting, Connection as QuinnConnection, Datagrams, IncomingUniStreams, NewConnection,
    RecvStream, SendStream, VarInt,
};
use std::collections::HashMap;
use tokio::sync::{
    mpsc::{Receiver as MpscReceiver, Sender as MpscSender},
    oneshot::Sender as OneshotSender,
};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, Response as TuicResponse};

pub struct Connection {
    controller: QuinnConnection,
    assoc_map: HashMap<u32, MpscSender<(Bytes, Address)>>,
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
}

impl Connection {
    pub async fn init(conn: Connecting, token_digest: [u8; 32], reduce_rtt: bool) -> Result<Self> {
        let NewConnection {
            connection,
            uni_streams,
            datagrams,
            ..
        } = if reduce_rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => conn.await?,
            }
        } else {
            conn.await?
        };

        tokio::spawn(Self::authenticate(connection.clone(), token_digest));
        tokio::spawn(Self::listen_incoming());

        Ok(Self {
            controller: connection,
            assoc_map: HashMap::new(),
            uni_streams,
            datagrams,
        })
    }

    pub fn handle_connect(
        &self,
        addr: Address,
        tx: OneshotSender<Option<(SendStream, RecvStream)>>,
    ) {
        let conn = self.controller.clone();

        tokio::spawn(async move {
            let res: Result<()> = try {
                let (mut send, mut recv) = conn.open_bi().await?;

                let addr = TuicAddress::from(addr);
                let cmd = TuicCommand::new_connect(addr);

                cmd.write_to(&mut send).await?;

                let resp = TuicResponse::read_from(&mut recv).await?;

                if resp.is_succeeded() {
                    let _ = tx.send(Some((send, recv)));
                    return;
                }
            };

            match res {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }

            let _ = tx.send(None);
        });
    }

    pub fn handle_associate(
        &self,
        assoc_id: u32,
        pkt_send_rx: MpscReceiver<(Bytes, Address)>,
        pkt_receive_tx: MpscSender<(Bytes, Address)>,
    ) {
    }

    pub async fn is_closed(&self) -> bool {
        self.controller.is_closed()
    }

    async fn authenticate(conn: QuinnConnection, token_digest: [u8; 32]) {
        let res: Result<()> = try {
            let mut stream = conn.open_uni().await?;
            let cmd = TuicCommand::new_authenticate(token_digest);
            cmd.write_to(&mut stream).await?;
        };

        match res {
            Ok(()) => {}
            Err(err) => {
                conn.close(VarInt::MAX, b"authentication failed");
                eprintln!("{err}");
            }
        }
    }

    async fn listen_incoming() {}
}
