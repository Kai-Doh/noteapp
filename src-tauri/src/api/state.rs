use std::sync::Arc;

use crate::auth::TokenCache;
use crate::db::pool::ReadPool;
use crate::db::writer::WriterHandle;

#[derive(Clone)]
pub struct AppState {
    pub writer: WriterHandle,
    pub ro_pool: ReadPool,
    pub token_cache: Arc<TokenCache>,
}
