use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError as QuinnConnectionError,
    IncomingBiStreams, NewConnection,
};
use thiserror::Error;

pub struct Connection {
    connection: QuinnConnection,
    bi_streams: IncomingBiStreams,
}

impl Connection {
    pub async fn new(conn: Connecting) -> Result<Self, ConnectionError> {
        let NewConnection {
            connection,
            bi_streams,
            ..
        } = conn.await?;
        Ok(Self {
            connection,
            bi_streams,
        })
    }

    pub async fn process(self) -> Result<(), ConnectionError> {
        /*let (mut auth_send, mut auth_recv) = conn.bi_streams.next().await.unwrap().unwrap();
        let _hs_req = tuic_protocol::HandshakeRequest::read_from(&mut auth_recv)
            .await
            .unwrap();

        let hs_res = tuic_protocol::HandshakeResponse::new(true);
        hs_res.write_to(&mut auth_send).await.unwrap();
        auth_send.finish().await.unwrap();

        while let Some(stream) = conn.bi_streams.next().await {
            match stream {
                Ok((mut send, mut recv)) => {
                    tokio::spawn(async move {
                        let connect_req = tuic_protocol::ConnectRequest::read_from(&mut recv)
                            .await
                            .unwrap();

                        let addr = connect_req
                            .address
                            .to_socket_addrs()
                            .unwrap()
                            .next()
                            .unwrap();
                        let mut stream = TcpStream::connect(addr).await.unwrap();

                        let connect_res = tuic_protocol::ConnectResponse::new(
                            tuic_protocol::Reply::Succeeded,
                        );
                        connect_res.write_to(&mut send).await.unwrap();

                        let (mut local_recv, mut local_send) = stream.split();

                        let remote_to_local = tokio::io::copy(&mut recv, &mut local_send);
                        let local_to_remote = tokio::io::copy(&mut local_recv, &mut send);
                        let _ = tokio::try_join!(remote_to_local, local_to_remote);
                    });
                }
                Err(ConnectionError::ApplicationClosed { .. }) => {
                    println!("Connection closed");
                    break;
                }
                Err(e) => {
                    println!("Connection error: {:?}", e);
                    break;
                }
            }
        }*/
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error(transparent)]
    Quinn(#[from] QuinnConnectionError),
}
