use super::{
    side::{self, Side},
    Assemblable, AssembleError, UdpSessions,
};
use crate::{Address, Header, Packet as PacketHeader};
use parking_lot::Mutex;
use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    marker::PhantomData,
    slice,
    sync::Arc,
};

pub struct Packet<M, B> {
    inner: Side<Tx, Rx<B>>,
    _marker: M,
}

struct Tx {
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

    /// Fragment the payload into multiple packets
    pub fn into_fragments<'a, P>(self, payload: P) -> Fragments<'a, P>
    where
        P: AsRef<[u8]> + 'a,
    {
        let Side::Tx(tx) = self.inner else { unreachable!() };
        Fragments::new(tx.assoc_id, tx.pkt_id, tx.addr, tx.max_pkt_size, payload)
    }

    /// Returns the UDP session ID
    pub fn assoc_id(&self) -> u16 {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        tx.assoc_id
    }

    /// Returns the packet ID
    pub fn pkt_id(&self) -> u16 {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        tx.pkt_id
    }

    /// Returns the address
    pub fn addr(&self) -> &Address {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.addr
    }
}

impl Debug for Packet<side::Tx, ()> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        f.debug_struct("Packet")
            .field("assoc_id", &tx.assoc_id)
            .field("pkt_id", &tx.pkt_id)
            .field("addr", &tx.addr)
            .field("max_pkt_size", &tx.max_pkt_size)
            .finish()
    }
}

struct Rx<B> {
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

    /// Reassembles the packet. If the packet is not complete yet, `None` is returned.
    pub fn assemble(self, data: B) -> Result<Option<Assemblable<B>>, AssembleError> {
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

    /// Returns the UDP session ID
    pub fn assoc_id(&self) -> u16 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.assoc_id
    }

    /// Returns the packet ID
    pub fn pkt_id(&self) -> u16 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.pkt_id
    }

    /// Returns the fragment ID
    pub fn frag_id(&self) -> u8 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.frag_id
    }

    /// Returns the total number of fragments
    pub fn frag_total(&self) -> u8 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.frag_total
    }

    /// Returns the address
    pub fn addr(&self) -> &Address {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        &rx.addr
    }

    /// Returns the size of the (fragmented) packet
    pub fn size(&self) -> u16 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.size
    }
}

impl<B> Debug for Packet<side::Rx, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        f.debug_struct("Packet")
            .field("assoc_id", &rx.assoc_id)
            .field("pkt_id", &rx.pkt_id)
            .field("frag_total", &rx.frag_total)
            .field("frag_id", &rx.frag_id)
            .field("size", &rx.size)
            .field("addr", &rx.addr)
            .finish()
    }
}

/// Iterator over fragments of a packet
#[derive(Debug)]
pub struct Fragments<'a, P> {
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

impl<'a, P> Fragments<'a, P>
where
    P: AsRef<[u8]> + 'a,
{
    fn new(assoc_id: u16, pkt_id: u16, addr: Address, max_pkt_size: usize, payload: P) -> Self {
        let header_addr_ref = Header::Packet(PacketHeader::new(0, 0, 0, 0, 0, addr));
        let header_addr_none_ref = Header::Packet(PacketHeader::new(0, 0, 0, 0, 0, Address::None));

        let first_frag_size = max_pkt_size - header_addr_ref.len();
        let frag_size_addr_none = max_pkt_size - header_addr_none_ref.len();

        let Header::Packet(pkt) = header_addr_ref else { unreachable!() };
        let (_, _, _, _, _, addr) = pkt.into();

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

impl<'a, P> Iterator for Fragments<'a, P>
where
    P: AsRef<[u8]> + 'a,
{
    type Item = (Header, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_frag_id < self.frag_total {
            let header_ref = Header::Packet(PacketHeader::new(0, 0, 0, 0, 0, self.addr.take()));

            let payload_size = self.max_pkt_size - header_ref.len();
            let next_frag_end =
                (self.next_frag_start + payload_size).min(self.payload.as_ref().len());

            let Header::Packet(pkt) = header_ref else { unreachable!() };
            let (_, _, _, _, _, addr) = pkt.into();

            let header = Header::Packet(PacketHeader::new(
                self.assoc_id,
                self.pkt_id,
                self.frag_total,
                self.next_frag_id,
                (next_frag_end - self.next_frag_start) as u16,
                addr,
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

impl<P> ExactSizeIterator for Fragments<'_, P>
where
    P: AsRef<[u8]>,
{
    fn len(&self) -> usize {
        self.frag_total as usize
    }
}
