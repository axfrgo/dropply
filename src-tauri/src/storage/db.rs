use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{Item, ItemType, LogEntry, PairingInfo};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "temp_store", "MEMORY")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn migrate(&self) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS items (
              id TEXT PRIMARY KEY,
              type TEXT NOT NULL,
              content_ref TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              device_id TEXT NOT NULL,
              name TEXT,
              mime_type TEXT,
              size_bytes INTEGER,
              sha256 TEXT,
              text_preview TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_items_updated_at ON items(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_items_sha256 ON items(sha256);

            CREATE TABLE IF NOT EXISTS sync_log (
              id TEXT PRIMARY KEY,
              device_id TEXT NOT NULL,
              item_id TEXT NOT NULL,
              op TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              payload TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sync_log_updated_at ON sync_log(updated_at DESC);

            CREATE TABLE IF NOT EXISTS pairing (
              device_id TEXT PRIMARY KEY,
              pairing_token TEXT NOT NULL,
              display_name TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn load_or_create_pairing(&self) -> AppResult<PairingInfo> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let existing = connection
            .query_row(
                "SELECT device_id, pairing_token, display_name FROM pairing LIMIT 1",
                [],
                |row| {
                    Ok(PairingInfo {
                        device_id: row.get(0)?,
                        pairing_token: row.get(1)?,
                        display_name: row.get(2)?,
                    })
                },
            )
            .optional()?;

        if let Some(pairing) = existing {
            return Ok(pairing);
        }

        let device_id = uuid::Uuid::new_v4().to_string();
        let pairing_token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(18)
            .map(char::from)
            .collect();
        let display_name = format!("Dropply {}", &device_id[..8]);

        connection.execute(
            "INSERT INTO pairing (device_id, pairing_token, display_name) VALUES (?1, ?2, ?3)",
            params![device_id, pairing_token, display_name],
        )?;

        Ok(PairingInfo {
            device_id,
            pairing_token,
            display_name,
        })
    }

    pub fn upsert_item(&self, item: &Item) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute(
            r#"
            INSERT INTO items (
              id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
              type = excluded.type,
              content_ref = excluded.content_ref,
              created_at = excluded.created_at,
              updated_at = CASE
                WHEN excluded.updated_at >= items.updated_at THEN excluded.updated_at
                ELSE items.updated_at
              END,
              device_id = excluded.device_id,
              name = excluded.name,
              mime_type = excluded.mime_type,
              size_bytes = excluded.size_bytes,
              sha256 = excluded.sha256,
              text_preview = excluded.text_preview
            "#,
            params![
                item.id,
                item_type_to_str(&item.item_type),
                item.content_ref,
                item.created_at.to_rfc3339(),
                item.updated_at.to_rfc3339(),
                item.device_id,
                item.name,
                item.mime_type,
                item.size_bytes,
                item.sha256,
                item.text_preview,
            ],
        )?;
        Ok(())
    }

    pub fn get_item(&self, item_id: &str) -> AppResult<Option<Item>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let item = connection
            .query_row(
                r#"
                SELECT id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview
                FROM items
                WHERE id = ?1
                "#,
                params![item_id],
                map_item,
            )
            .optional()?;
        Ok(item)
    }

    pub fn list_items(&self) -> AppResult<Vec<Item>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let mut stmt = connection.prepare(
            r#"
            SELECT id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview
            FROM items
            ORDER BY updated_at DESC
            LIMIT 500
            "#,
        )?;
        let items = stmt
            .query_map([], map_item)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn count_items(&self) -> AppResult<i64> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let count = connection.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn delete_item(&self, item_id: &str) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute("DELETE FROM items WHERE id = ?1", params![item_id])?;
        Ok(())
    }

    pub fn count_items_with_content_ref(&self, content_ref: &str) -> AppResult<i64> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let count = connection.query_row(
            "SELECT COUNT(*) FROM items WHERE content_ref = ?1",
            params![content_ref],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn append_log(&self, entry: &LogEntry) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute(
            "INSERT INTO sync_log (id, device_id, item_id, op, updated_at, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.id,
                entry.device_id,
                entry.item_id,
                entry.op,
                entry.updated_at.to_rfc3339(),
                entry.payload.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn pending_log_entries(&self, limit: usize) -> AppResult<Vec<LogEntry>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let mut stmt = connection.prepare(
            "SELECT id, device_id, item_id, op, updated_at, payload FROM sync_log ORDER BY updated_at ASC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                let payload_str: String = row.get(5)?;
                Ok(LogEntry {
                    id: row.get(0)?,
                    device_id: row.get(1)?,
                    item_id: row.get(2)?,
                    op: row.get(3)?,
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .map(|value| value.with_timezone(&Utc))
                        .map_err(|err| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(err),
                            )
                        })?,
                    payload: serde_json::from_str(&payload_str).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}

fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        item_type: str_to_item_type(&row.get::<_, String>(1)?),
        content_ref: row.get(2)?,
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?,
        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?,
        device_id: row.get(5)?,
        name: row.get(6)?,
        mime_type: row.get(7)?,
        size_bytes: row.get(8)?,
        sha256: row.get(9)?,
        text_preview: row.get(10)?,
    })
}

fn item_type_to_str(item_type: &ItemType) -> &'static str {
    match item_type {
        ItemType::Text => "text",
        ItemType::Image => "image",
        ItemType::File => "file",
    }
}

fn str_to_item_type(value: &str) -> ItemType {
    match value {
        "image" => ItemType::Image,
        "file" => ItemType::File,
        _ => ItemType::Text,
    }
}
