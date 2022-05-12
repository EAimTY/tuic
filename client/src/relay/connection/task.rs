use super::UdpSessionMap;
use crate::relay::{Address, BiStream, RelayError};
use bytes::{Bytes, BytesMut};
use quinn::Connection as QuinnConnection;
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, sync::oneshot::Sender};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand};

pub async fn connect(
    conn: QuinnConnection,
    addr: Address,
    tx: Sender<BiStream>,
) -> Result<(), RelayError> {
    async fn get_streams(
        conn: QuinnConnection,
        addr: Address,
    ) -> Result<Option<BiStream>, RelayError> {
        let (send, recv) = conn.open_bi().await?;
        let mut stream = BiStream::new(send, recv);
        let addr = TuicAddress::from(addr);

        let cmd = TuicCommand::new_connect(addr);
        cmd.write_to(&mut stream).await?;

        let resp = match TuicCommand::read_from(&mut stream).await {
            Ok(resp) => resp,
            Err(err) => {
                let _ = stream.shutdown().await;
                return Err(RelayError::Protocol(err));
            }
        };

        if let TuicCommand::Response(true) = resp {
            Ok(Some(stream))
        } else {
            let _ = stream.shutdown().await;
            Ok(None)
        }
    }

    match get_streams(conn, addr).await {
        Ok(Some(stream)) => {
            let _ = tx.send(stream);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(err) => Err(err),
    }
}

pub async fn packet_to_uni_stream(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), RelayError> {
    let mut stream = conn.open_uni().await?;

    let addr = TuicAddress::from(addr);
    let cmd = TuicCommand::new_packet(assoc_id, pkt.len() as u16, addr);

    cmd.write_to(&mut stream).await?;
    stream.write_all(&pkt).await?;
    let _ = stream.shutdown().await;

    Ok(())
}

pub async fn packet_to_datagram(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), RelayError> {
    let addr = TuicAddress::from(addr);
    let cmd = TuicCommand::new_packet(assoc_id, pkt.len() as u16, addr);

    let mut buf = BytesMut::with_capacity(cmd.serialized_len());
    cmd.write_to_buf(&mut buf);
    buf.extend_from_slice(&pkt);

    let pkt = buf.freeze();
    conn.send_datagram(pkt)?;

    Ok(())
}

pub async fn packet_from_server(
    pkt: Bytes,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    addr: Address,
) -> Result<(), RelayError> {
    let recv_pkt_tx = udp_sessions
        .lock()
        .get(&assoc_id)
        .cloned()
        .ok_or(RelayError::UdpSessionNotFound(assoc_id))?;

    let _ = recv_pkt_tx.send((pkt, addr)).await;

    Ok(())
}

pub async fn dissociate(conn: QuinnConnection, assoc_id: u32) -> Result<(), RelayError> {
    let mut stream = conn.open_uni().await?;
    let cmd = TuicCommand::new_dissociate(assoc_id);
    cmd.write_to(&mut stream).await?;
    let _ = stream.shutdown().await;

    Ok(())
}
