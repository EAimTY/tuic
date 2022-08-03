use super::stream::{RecvStream, StreamReg};
use crate::{
    common::{PacketBuffer, PacketBufferError},
    protocol::{Command, Error as TuicError},
    Packet, UdpRelayMode,
};
use bytes::Bytes;
use futures_util::StreamExt;
use quinn::{ConnectionError as QuinnConnectionError, Datagrams, IncomingUniStreams};
use std::{
    io::{Error as IoError, ErrorKind},
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{io::AsyncReadExt, time};

#[derive(Debug)]
pub struct IncomingPackets {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
    pkt_buf: PacketBuffer,
    last_gc_time: Instant,
}

impl IncomingPackets {
    pub(super) fn new(
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        udp_relay_mode: UdpRelayMode,
        stream_reg: Arc<StreamReg>,
    ) -> Self {
        Self {
            uni_streams,
            datagrams,
            udp_relay_mode,
            stream_reg,
            pkt_buf: PacketBuffer::new(),
            last_gc_time: Instant::now(),
        }
    }

    pub async fn accept(
        &mut self,
        gc_interval: Duration,
        gc_timeout: Duration,
    ) -> Option<Result<Packet, IncomingPacketsError>> {
        match self.udp_relay_mode {
            UdpRelayMode::Native => self.accept_from_datagrams(gc_interval, gc_timeout).await,
            UdpRelayMode::Quic => self.accept_from_uni_streams().await,
        }
    }

    async fn accept_from_datagrams(
        &mut self,
        gc_interval: Duration,
        gc_timeout: Duration,
    ) -> Option<Result<Packet, IncomingPacketsError>> {
        #[inline]
        async fn process_datagram(
            pkt_buf: &mut PacketBuffer,
            dg: Result<Bytes, IncomingPacketsError>,
        ) -> Result<Option<Packet>, IncomingPacketsError> {
            let dg = dg?;
            let cmd = Command::read_from(&mut dg.as_ref()).await?;
            let cmd_len = cmd.serialized_len();

            match cmd {
                Command::Packet {
                    assoc_id,
                    pkt_id,
                    frag_total,
                    frag_id,
                    len,
                    addr,
                } => {
                    if let Some(pkt) = pkt_buf.insert(
                        assoc_id,
                        pkt_id,
                        frag_total,
                        frag_id,
                        addr,
                        dg.slice(cmd_len..cmd_len + len as usize),
                    )? {
                        Ok(Some(pkt))
                    } else {
                        Ok(None)
                    }
                }
                cmd => Err(IncomingPacketsError::Tuic(TuicError::InvalidCommand(
                    cmd.as_type_code(),
                ))),
            }
        }

        if self.last_gc_time.elapsed() > gc_interval {
            self.pkt_buf.collect_garbage(gc_timeout);
            self.last_gc_time = Instant::now();
        }

        let mut gc_interval = time::interval(gc_interval);

        loop {
            tokio::select! {
                dg = self.datagrams.next() => {
                    if let Some(dg) = dg {
                        let dg = dg.map_err(IncomingPacketsError::from_quinn_connection_error);
                        match process_datagram(&mut self.pkt_buf, dg).await {
                            Ok(Some(pkt)) => break Some(Ok(pkt)),
                            Ok(None) => {}
                            Err(err) => break Some(Err(err)),
                        }
                    } else {
                        break None;
                    }
                }
                _ = gc_interval.tick() => {
                    self.pkt_buf.collect_garbage(gc_timeout);
                    self.last_gc_time = Instant::now();
                }
            }
        }
    }

    async fn accept_from_uni_streams(&mut self) -> Option<Result<Packet, IncomingPacketsError>> {
        #[inline]
        async fn process_uni_stream(
            recv: Result<RecvStream, IncomingPacketsError>,
        ) -> Result<Packet, IncomingPacketsError> {
            let mut recv = recv?;
            let cmd = Command::read_from(&mut recv).await?;

            match cmd {
                Command::Packet {
                    assoc_id,
                    pkt_id,
                    frag_total,
                    frag_id,
                    len,
                    addr,
                } => {
                    if frag_id != 0 || frag_total != 1 {
                        return Err(IncomingPacketsError::BadFragment);
                    }

                    if addr.is_none() {
                        return Err(IncomingPacketsError::NoAddress);
                    }

                    let mut buf = vec![0; len as usize];
                    recv.read_exact(&mut buf).await?;
                    let pkt = Bytes::from(buf);

                    Ok(Packet::new(assoc_id, pkt_id, addr.unwrap(), pkt))
                }
                _ => Err(IncomingPacketsError::Tuic(TuicError::InvalidCommand(
                    cmd.as_type_code(),
                ))),
            }
        }

        if let Some(recv) = self.uni_streams.next().await {
            let recv = recv
                .map(|recv| RecvStream::new(recv, self.stream_reg.as_ref().clone()))
                .map_err(IncomingPacketsError::from_quinn_connection_error);
            Some(process_uni_stream(recv).await)
        } else {
            None
        }
    }
}

#[derive(Error, Debug)]
pub enum IncomingPacketsError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Tuic(TuicError),
    #[error("received bad-fragmented packet")]
    BadFragment,
    #[error("missing address in packet with frag_id 0")]
    NoAddress,
    #[error("unexpected address in packet")]
    UnexpectedAddress,
}

impl IncomingPacketsError {
    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }
}

impl From<PacketBufferError> for IncomingPacketsError {
    #[inline]
    fn from(err: PacketBufferError) -> Self {
        match err {
            PacketBufferError::NoAddress => Self::NoAddress,
            PacketBufferError::UnexpectedAddress => Self::UnexpectedAddress,
            PacketBufferError::BadFragment => Self::BadFragment,
        }
    }
}

impl From<TuicError> for IncomingPacketsError {
    #[inline]
    fn from(err: TuicError) -> Self {
        match err {
            TuicError::Io(err) => Self::Io(err),
            err => Self::Tuic(err),
        }
    }
}

impl From<IncomingPacketsError> for IoError {
    #[inline]
    fn from(err: IncomingPacketsError) -> Self {
        match err {
            IncomingPacketsError::Io(err) => Self::from(err),
            err => Self::new(ErrorKind::Other, err),
        }
    }
}
