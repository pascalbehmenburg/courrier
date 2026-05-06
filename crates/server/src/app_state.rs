use courrier_core::{sync::SyncCoordinator, Database, Encryptor};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub encryptor: Encryptor,
    pub coordinator: Arc<SyncCoordinator>,
}
