use super::Command;

// +--------+-----------+
// | METHOD |    OPT    |
// +--------+-----------+
// |   1    | Variable  |
// +--------+-----------+
#[derive(Clone, Debug)]
pub struct Authenticate<A>
where
    A: Method,
{
    pub method: A,
}

impl<A> Authenticate<A>
where
    A: Method,
{
    pub(super) const TYPE_CODE: u8 = 0x00;

    pub fn new(method: A) -> Self {
        Self { method }
    }
}

impl<A> Command for Authenticate<A>
where
    A: Method,
{
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        1 + self.method.len()
    }
}

pub trait Method {
    fn type_code(&self) -> u8;
    fn len(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct None;

impl None {
    const TYPE_CODE: u8 = 0x00;

    pub fn new() -> Self {
        Self
    }
}

impl Method for None {
    fn type_code(&self) -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        0
    }
}

#[derive(Debug, Clone)]
pub struct Blake3 {
    pub token: [u8; 32],
}

impl Blake3 {
    const TYPE_CODE: u8 = 0x01;

    pub fn new(token: [u8; 32]) -> Self {
        Self { token }
    }
}

impl Method for Blake3 {
    fn type_code(&self) -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        32
    }
}
