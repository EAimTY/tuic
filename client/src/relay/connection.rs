use anyhow::Result;
use quinn::{
    Connecting, Connection as QuinnConnection, Datagrams, IncomingUniStreams, NewConnection, VarInt,
};
use tuic_protocol::Command as TuicCommand;

pub struct Connection {
    pub controller: QuinnConnection,
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

        Ok(Self {
            controller: connection,
            uni_streams,
            datagrams,
        })
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
