use super::{
    side::{self, Side},
    AssembleError, Assembled, UdpSessions,
};
use crate::protocol::{Address, Header, Packet as PacketHeader};
use parking_lot::Mutex;
use std::{marker::PhantomData, slice, sync::Arc};

pub struct Packet<M, B> {
    inner: Side<Tx, Rx<B>>,
    _marker: M,
}

pub struct Tx {
    assoc_id: u16,
    pkt_id: u16,
    addr: Address,
    max_pkt_size: usize,
}

impl<B> Packet<side::Tx, B> {
    pub(super) fn new(assoc_id: u16, pkt_id: u16, addr: Address, max_pkt_size: usize) -> Self {
        Self {
            inner: Side::Tx(Tx {
                assoc_id,
                pkt_id,
                addr,
                max_pkt_size,
            }),
            _marker: side::Tx,
        }
    }

    pub fn into_fragments<'a, P>(self, payload: P) -> Fragment<'a, P>
    where
        P: AsRef<[u8]>,
    {
        let Side::Tx(tx) = self.inner else { unreachable!() };
        Fragment::new(tx.assoc_id, tx.pkt_id, tx.addr, tx.max_pkt_size, payload)
    }
}

pub struct Rx<B> {
    sessions: Arc<Mutex<UdpSessions<B>>>,
    assoc_id: u16,
    pkt_id: u16,
    frag_total: u8,
    frag_id: u8,
    size: u16,
    addr: Address,
}

impl<B> Packet<side::Rx, B>
where
    B: AsRef<[u8]>,
{
    pub(super) fn new(
        sessions: Arc<Mutex<UdpSessions<B>>>,
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
    ) -> Self {
        Self {
            inner: Side::Rx(Rx {
                sessions,
                assoc_id,
                pkt_id,
                frag_total,
                frag_id,
                size,
                addr,
            }),
            _marker: side::Rx,
        }
    }

    pub fn assemble<A>(self, data: B) -> Result<Option<(A, Address)>, AssembleError>
    where
        A: Assembled<B>,
    {
        let Side::Rx(rx) = self.inner else { unreachable!() };
        let mut sessions = rx.sessions.lock();

        sessions.insert(
            rx.assoc_id,
            rx.pkt_id,
            rx.frag_total,
            rx.frag_id,
            rx.size,
            rx.addr,
            data,
        )
    }
}

pub struct Fragment<'a, P>
where
    P: 'a,
{
    assoc_id: u16,
    pkt_id: u16,
    addr: Address,
    max_pkt_size: usize,
    frag_total: u8,
    next_frag_id: u8,
    next_frag_start: usize,
    payload: P,
    _marker: PhantomData<&'a P>,
}

impl<'a, P> Fragment<'a, P>
where
    P: AsRef<[u8]> + 'a,
{
    fn new(assoc_id: u16, pkt_id: u16, addr: Address, max_pkt_size: usize, payload: P) -> Self {
        let first_frag_size = max_pkt_size - PacketHeader::len_without_addr() - addr.len();
        let frag_size_addr_none =
            max_pkt_size - PacketHeader::len_without_addr() - Address::None.len();

        let frag_total = if first_frag_size < payload.as_ref().len() {
            (1 + (payload.as_ref().len() - first_frag_size) / frag_size_addr_none + 1) as u8
        } else {
            1u8
        };

        Self {
            assoc_id,
            pkt_id,
            addr,
            max_pkt_size,
            frag_total,
            next_frag_id: 0,
            next_frag_start: 0,
            payload,
            _marker: PhantomData,
        }
    }
}

impl<'a, P> Iterator for Fragment<'a, P>
where
    P: AsRef<[u8]> + 'a,
{
    type Item = (Header, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_frag_id < self.frag_total {
            let payload_size =
                self.max_pkt_size - PacketHeader::len_without_addr() - self.addr.len();
            let next_frag_end =
                (self.next_frag_start + payload_size).min(self.payload.as_ref().len());

            let header = Header::Packet(PacketHeader::new(
                self.assoc_id,
                self.pkt_id,
                self.frag_total,
                self.next_frag_id,
                (next_frag_end - self.next_frag_start) as u16,
                self.addr.take(),
            ));

            let payload_ptr = &(self.payload.as_ref()[self.next_frag_start]) as *const u8;
            let payload =
                unsafe { slice::from_raw_parts(payload_ptr, next_frag_end - self.next_frag_start) };

            self.next_frag_id += 1;
            self.next_frag_start = next_frag_end;

            Some((header, payload))
        } else {
            None
        }
    }
}

impl<P> ExactSizeIterator for Fragment<'_, P>
where
    P: AsRef<[u8]>,
{
    fn len(&self) -> usize {
        self.frag_total as usize
    }
}
