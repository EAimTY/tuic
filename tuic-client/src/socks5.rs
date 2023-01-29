use crate::error::Error;
use bytes::Bytes;
use tuic::Address;

pub async fn recv_pkt(pkt: Bytes, addr: Address, assoc_id: u16) -> Result<(), Error> {
    todo!()
}
