use std::sync::Arc;

#[derive(Clone)]
pub struct Register(Arc<()>);

impl Register {
    fn new() -> Self {
        Self(Arc::new(()))
    }

    pub fn count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}
