use super::side::{self, Side};
use crate::{Authenticate as AuthenticateHeader, Header};
use std::fmt::{Debug, Formatter, Result as FmtResult};
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
    pub(super) fn new(
        uuid: Uuid,
        password: impl AsRef<[u8]>,
        exporter: &impl KeyingMaterialExporter,
    ) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Authenticate(AuthenticateHeader::new(
                    uuid,
                    exporter.export_keying_material(uuid.as_ref(), password.as_ref()),
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

impl Debug for Authenticate<side::Tx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        f.debug_struct("Authenticate")
            .field("header", &tx.header)
            .finish()
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
    pub fn is_valid(
        &self,
        password: impl AsRef<[u8]>,
        exporter: &impl KeyingMaterialExporter,
    ) -> bool {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.token == exporter.export_keying_material(rx.uuid.as_ref(), password.as_ref())
    }
}

impl Debug for Authenticate<side::Rx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        f.debug_struct("Authenticate")
            .field("uuid", &rx.uuid)
            .field("token", &rx.token)
            .finish()
    }
}

/// The trait for exporting keying material
pub trait KeyingMaterialExporter {
    /// Exports keying material
    fn export_keying_material(&self, label: &[u8], context: &[u8]) -> [u8; 32];
}
