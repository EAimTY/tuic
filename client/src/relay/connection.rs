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
    pub controller: QuinnConnection,
    pub assoc_map: HashMap<u32, MpscSender<(Bytes, Address)>>,
    pub uni_streams: IncomingUniStreams,
    pub datagrams: Datagrams,
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
    }

    pub fn handle_associate(
        &self,
        assoc_id: u32,
        pkt_send_rx: MpscReceiver<(Bytes, Address)>,
        pkt_receive_tx: MpscSender<(Bytes, Address)>,
    ) {
    }

    pub async fn is_closed(&self) -> bool {
        self.controller.open_uni().await.is_err()
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

async fn handle_command_connect(
    mut send: SendStream,
    mut recv: RecvStream,
    addr: Address,
    tx: OneshotSender<Option<(SendStream, RecvStream)>>,
) {
    let addr = TuicAddress::from(addr);
    let cmd = TuicCommand::new_connect(addr);

    match cmd.write_to(&mut send).await {
        Ok(()) => match TuicResponse::read_from(&mut recv).await {
            Ok(res) => {
                if res.is_succeeded() {
                    let _ = tx.send(Some((send, recv)));
                    return;
                }
            }
            Err(err) => eprintln!("{err}"),
        },
        Err(err) => eprintln!("{err}"),
    }

    let _ = tx.send(None);
}
