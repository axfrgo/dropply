use crate::storage::Storage;
use crate::sync::SyncManager;

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub sync: SyncManager,
}

