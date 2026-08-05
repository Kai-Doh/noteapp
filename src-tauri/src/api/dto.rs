use serde::Serialize;

use crate::db::writer::WriteResult;

/// Shared response shape for any write route — used by nodes, memory
/// (hot_memory/user_profile), and review routes.
#[derive(Serialize)]
pub struct WriteResultDto {
    pub id: String,
    pub revision_number: Option<i64>,
}

impl From<WriteResult> for WriteResultDto {
    fn from(result: WriteResult) -> Self {
        WriteResultDto {
            id: result.outcome.entity_id,
            revision_number: result.revision_number,
        }
    }
}
