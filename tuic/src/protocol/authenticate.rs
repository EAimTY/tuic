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
    const CMD_TYPE: u8 = 0x00;

    pub fn new(method: A) -> Self {
        Self { method }
    }

    pub const fn cmd_type() -> u8 {
        Self::CMD_TYPE
    }

    pub fn len(&self) -> usize {
        1 + self.method.len()
    }
}

pub trait Method {
    fn auth_type(&self) -> u8;
    fn len(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct None;

impl None {
    const AUTH_TYPE: u8 = 0x00;

    pub fn new() -> Self {
        Self
    }
}

impl Method for None {
    fn auth_type(&self) -> u8 {
        Self::AUTH_TYPE
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
    const AUTH_TYPE: u8 = 0x01;

    pub fn new(token: [u8; 32]) -> Self {
        Self { token }
    }
}

impl Method for Blake3 {
    fn auth_type(&self) -> u8 {
        Self::AUTH_TYPE
    }

    fn len(&self) -> usize {
        32
    }
}