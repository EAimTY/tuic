use super::side::{self, Side};
use crate::protocol::{Authenticate as AuthenticateHeader, Header};
use uuid::Uuid;

/// The model of the `Authenticate` command
pub struct Authenticate<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

struct Tx {
    header: Header,
}

impl Authenticate<side::Tx> {
    pub(super) fn new(uuid: Uuid, exporter: impl KeyingMaterialExporter) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Authenticate(AuthenticateHeader::new(
                    uuid,
                    exporter.export_keying_material(uuid.as_ref(), uuid.as_ref()),
                )),
            }),
            _marker: side::Tx,
        }
    }

    /// Returns the header of the `Authenticate` command
    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}

struct Rx {
    uuid: Uuid,
    token: [u8; 32],
}

impl Authenticate<side::Rx> {
    pub(super) fn new(uuid: Uuid, token: [u8; 32]) -> Self {
        Self {
            inner: Side::Rx(Rx { uuid, token }),
            _marker: side::Rx,
        }
    }

    /// Returns the UUID of the peer
    pub fn uuid(&self) -> Uuid {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.uuid
    }

    /// Returns the token of the peer
    pub fn token(&self) -> [u8; 32] {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.token
    }

    /// Returns whether the token is valid
    pub fn is_valid(&self, exporter: impl KeyingMaterialExporter) -> bool {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.token == exporter.export_keying_material(rx.uuid.as_ref(), rx.uuid.as_ref())
    }
}

/// The trait for exporting keying material
pub trait KeyingMaterialExporter {
    /// Exports keying material
    fn export_keying_material(&self, label: &[u8], context: &[u8]) -> [u8; 32];
}
