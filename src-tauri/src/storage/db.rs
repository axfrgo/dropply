use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::AppResult;
use crate::models::{IntentState, Item, ItemType, LogEntry, PairingInfo};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        let connection = Connection::open(path).map_err(|error| {
            crate::error::AppError::Message(format!(
                "sqlite connection open failed for {}: {}",
                path.display(),
                error
            ))
        })?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(|error| {
            crate::error::AppError::Message(format!(
                "sqlite pragma journal_mode=WAL failed for {}: {}",
                path.display(),
                error
            ))
        })?;
        connection.pragma_update(None, "synchronous", "NORMAL").map_err(|error| {
            crate::error::AppError::Message(format!(
                "sqlite pragma synchronous=NORMAL failed for {}: {}",
                path.display(),
                error
            ))
        })?;
        connection.pragma_update(None, "temp_store", "MEMORY").map_err(|error| {
            crate::error::AppError::Message(format!(
                "sqlite pragma temp_store=MEMORY failed for {}: {}",
                path.display(),
                error
            ))
        })?;
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
              text_preview TEXT,
              source_context_json TEXT,
              semantic_context_json TEXT,
              suggested_actions_json TEXT,
              intent_state TEXT NOT NULL DEFAULT 'captured',
              trust_context_json TEXT
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
        add_column_if_missing(&connection, "items", "source_context_json", "TEXT")?;
        add_column_if_missing(&connection, "items", "semantic_context_json", "TEXT")?;
        add_column_if_missing(&connection, "items", "suggested_actions_json", "TEXT")?;
        add_column_if_missing(
            &connection,
            "items",
            "intent_state",
            "TEXT NOT NULL DEFAULT 'captured'",
        )?;
        add_column_if_missing(&connection, "items", "trust_context_json", "TEXT")?;
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
        
        let mut rng = rand::thread_rng();
        let part1: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        let part2: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        let part3: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        
        let pairing_token = format!("ZN-{}-{}-{}", part1.to_uppercase(), part2.to_uppercase(), part3.to_uppercase());
        
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

    pub fn update_pairing_token(&self, new_token: &str) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute(
            "UPDATE pairing SET pairing_token = ?1",
            params![new_token],
        )?;
        Ok(())
    }

    pub fn reset_pairing_token(&self) -> AppResult<String> {
        let mut rng = rand::thread_rng();
        let part1: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        let part2: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        let part3: String = (&mut rng).sample_iter(&Alphanumeric).take(4).map(char::from).collect();
        let new_token = format!("ZN-{}-{}-{}", part1.to_uppercase(), part2.to_uppercase(), part3.to_uppercase());

        self.update_pairing_token(&new_token)?;
        Ok(new_token)
    }

    pub fn clear_pairing(&self) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        connection.execute("DELETE FROM pairing", [])?;
        connection.execute("DELETE FROM items", [])?;
        connection.execute("DELETE FROM sync_log", [])?;
        Ok(())
    }

    pub fn upsert_item(&self, item: &Item) -> AppResult<()> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let source_context_json = serialize_optional_json(&item.source_context)?;
        let semantic_context_json = serialize_optional_json(&item.semantic_context)?;
        let suggested_actions_json = serde_json::to_string(&item.suggested_actions)?;
        let trust_context_json = serialize_optional_json(&item.trust_context)?;
        connection.execute(
            r#"
            INSERT INTO items (
              id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview,
              source_context_json, semantic_context_json, suggested_actions_json, intent_state, trust_context_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
              text_preview = excluded.text_preview,
              source_context_json = excluded.source_context_json,
              semantic_context_json = excluded.semantic_context_json,
              suggested_actions_json = excluded.suggested_actions_json,
              intent_state = excluded.intent_state,
              trust_context_json = excluded.trust_context_json
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
                source_context_json,
                semantic_context_json,
                suggested_actions_json,
                item.intent_state.as_str(),
                trust_context_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_item(&self, item_id: &str) -> AppResult<Option<Item>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let item = connection
            .query_row(
                r#"
                SELECT id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview,
                       source_context_json, semantic_context_json, suggested_actions_json, intent_state, trust_context_json
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
            SELECT id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview,
                   source_context_json, semantic_context_json, suggested_actions_json, intent_state, trust_context_json
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

    pub fn find_latest_item_by_content_ref_and_device(
        &self,
        content_ref: &str,
        device_id: &str,
    ) -> AppResult<Option<Item>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let item = connection
            .query_row(
                r#"
                SELECT id, type, content_ref, created_at, updated_at, device_id, name, mime_type, size_bytes, sha256, text_preview,
                       source_context_json, semantic_context_json, suggested_actions_json, intent_state, trust_context_json
                FROM items
                WHERE content_ref = ?1 AND device_id = ?2
                ORDER BY updated_at DESC
                LIMIT 1
                "#,
                params![content_ref, device_id],
                map_item,
            )
            .optional()?;
        Ok(item)
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

    pub fn list_deleted_log_entries(&self) -> AppResult<Vec<LogEntry>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let mut stmt = connection.prepare(
            "SELECT id, device_id, item_id, op, updated_at, payload FROM sync_log WHERE op = 'delete' ORDER BY updated_at DESC",
        )?;
        let entries = stmt
            .query_map([], |row| {
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

    pub fn list_recent_logs(&self, limit: usize) -> AppResult<Vec<LogEntry>> {
        let connection = self.connection.lock().expect("db mutex poisoned");
        let mut stmt = connection.prepare(
            "SELECT id, device_id, item_id, op, updated_at, payload FROM sync_log ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map([limit as i64], |row| {
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
        source_context: parse_optional_json(row, 11)?,
        semantic_context: parse_optional_json(row, 12)?,
        suggested_actions: parse_json_or_default(row, 13)?,
        intent_state: row
            .get::<_, Option<String>>(14)?
            .as_deref()
            .map(IntentState::from_str)
            .unwrap_or_default(),
        trust_context: parse_optional_json(row, 15)?,
    })
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> AppResult<()> {
    let mut stmt = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);

    if !exists {
        connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"), [])?;
    }

    Ok(())
}

fn serialize_optional_json<T: Serialize>(value: &Option<T>) -> AppResult<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn parse_optional_json<T: DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<T>> {
    let value = row.get::<_, Option<String>>(index)?;
    value
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })
        })
        .transpose()
}

fn parse_json_or_default<T: DeserializeOwned + Default>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let Some(value) = row.get::<_, Option<String>>(index)? else {
        return Ok(T::default());
    };

    serde_json::from_str(&value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(err))
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
