use chrono::Utc;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::LogEntry;
use crate::storage::db::Database;

#[derive(Clone)]
pub struct LogStore {
    db: Database,
}

impl LogStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn append(&self, op: &str, item_id: &str, payload: serde_json::Value) -> AppResult<()> {
        let entry = LogEntry {
            id: Uuid::new_v4().to_string(),
            device_id: payload
                .get("device_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            item_id: item_id.to_string(),
            op: op.to_string(),
            updated_at: Utc::now(),
            payload,
        };
        self.db.append_log(&entry)?;
        Ok(())
    }

    pub fn pending(&self, limit: usize) -> AppResult<Vec<LogEntry>> {
        self.db.pending_log_entries(limit)
    }
}

