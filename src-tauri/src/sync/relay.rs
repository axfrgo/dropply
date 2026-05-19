#[derive(Clone, Debug, Default)]
pub struct RelayTransport {
    pub connected: bool,
}

impl RelayTransport {
    pub fn new() -> Self {
        Self { connected: false }
    }
}
