use self::associate::AssociateMap;
use super::Address;
use crate::config::UdpMode;
use anyhow::Result;
use quinn::{Connecting, Connection as QuinnConnection, NewConnection, VarInt};
use std::sync::Arc;
use tuic_protocol::Command as TuicCommand;

mod associate;
mod connect;

pub struct Connection {
    controller: QuinnConnection,
    udp_mode: UdpMode,
    assoc_map: Arc<AssociateMap>,
}

impl Connection {
    pub async fn init(
        conn: Connecting,
        token_digest: [u8; 32],
        udp_mode: UdpMode,
        reduce_rtt: bool,
    ) -> Result<Self> {
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

        let assoc_map = Arc::new(AssociateMap::new());

        tokio::spawn(Self::authenticate(connection.clone(), token_digest));
        tokio::spawn(Self::listen_incoming(
            uni_streams,
            datagrams,
            assoc_map.clone(),
        ));

        Ok(Self {
            controller: connection,
            udp_mode,
            assoc_map,
        })
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
}
