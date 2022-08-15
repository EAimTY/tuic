use super::stream::{RecvStream, Stream};
use crate::protocol::{Address, Command, MarshalingError};
use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct Packet {
    pub id: u16,
    pub associate_id: u32,
    pub address: Address,
    pub data: Bytes,
}

impl Packet {
    pub(crate) fn new(assoc_id: u32, pkt_id: u16, addr: Address, pkt: Bytes) -> Self {
        Self {
            id: pkt_id,
            associate_id: assoc_id,
            address: addr,
            data: pkt,
        }
    }
}

pub(crate) enum TaskSource {
    BiStream(Stream),
    UniStream(RecvStream),
    Datagram(Bytes),
}

impl TaskSource {
    pub(crate) async fn accept(self) -> Result<RawTask, MarshalingError> {
        match self {
            TaskSource::BiStream(mut bi_stream) => Ok(RawTask::new(
                Command::read_from(&mut bi_stream).await?,
                RawTaskPayload::BiStream(bi_stream),
            )),
            TaskSource::UniStream(mut uni_stream) => Ok(RawTask::new(
                Command::read_from(&mut uni_stream).await?,
                RawTaskPayload::UniStream(uni_stream),
            )),
            TaskSource::Datagram(datagram) => {
                let cmd = Command::read_from(&mut datagram.as_ref()).await?;
                let payload = datagram.slice(cmd.serialized_len()..);
                Ok(RawTask::new(cmd, RawTaskPayload::Datagram(payload)))
            }
        }
    }
}

pub(crate) struct RawTask {
    header: Command,
    payload: RawTaskPayload,
}

impl RawTask {
    pub(crate) fn new(header: Command, payload: RawTaskPayload) -> Self {
        Self { header, payload }
    }
}

pub(crate) enum RawTaskPayload {
    BiStream(Stream),
    UniStream(RecvStream),
    Datagram(Bytes),
}
