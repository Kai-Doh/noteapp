use chrono::Utc;
use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::writer::WriteError;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PropertyInput {
    pub key: String,
    pub value_type: String,
    pub value_text: Option<String>,
    pub value_number: Option<f64>,
    pub value_bool: Option<bool>,
    pub value_date: Option<String>,
    pub value_node_id: Option<String>,
}

const ALLOWED_VALUE_TYPES: [&str; 5] = ["text", "number", "bool", "date", "node_ref"];

fn validate(p: &PropertyInput) -> Result<(), WriteError> {
    if !ALLOWED_VALUE_TYPES.contains(&p.value_type.as_str()) {
        return Err(WriteError::Invalid(format!(
            "unknown property value_type '{}'",
            p.value_type
        )));
    }
    if p.key.trim().is_empty() {
        return Err(WriteError::Invalid("property key must not be empty".into()));
    }
    Ok(())
}

/// Upserts each property row (keyed on `(node_id, key)`, matching the schema's
/// UNIQUE constraint). Called from within a node create/update mutation closure —
/// never its own top-level write.
pub fn upsert_properties(
    txn: &Transaction,
    node_id: &str,
    properties: &[PropertyInput],
) -> Result<(), WriteError> {
    let now = Utc::now().to_rfc3339();
    for p in properties {
        validate(p)?;
        let existing_id: Option<String> = txn
            .query_row(
                "SELECT id FROM properties WHERE node_id = ?1 AND key = ?2",
                params![node_id, p.key],
                |row| row.get(0),
            )
            .ok();
        match existing_id {
            Some(id) => {
                txn.execute(
                    "UPDATE properties SET value_type = ?1, value_text = ?2, value_number = ?3,
                        value_bool = ?4, value_date = ?5, value_node_id = ?6, updated_at = ?7
                     WHERE id = ?8",
                    params![
                        p.value_type,
                        p.value_text,
                        p.value_number,
                        p.value_bool,
                        p.value_date,
                        p.value_node_id,
                        now,
                        id
                    ],
                )?;
            }
            None => {
                let id = Uuid::new_v4().to_string();
                txn.execute(
                    "INSERT INTO properties (
                        id, node_id, key, value_type, value_text, value_number,
                        value_bool, value_date, value_node_id, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        id,
                        node_id,
                        p.key,
                        p.value_type,
                        p.value_text,
                        p.value_number,
                        p.value_bool,
                        p.value_date,
                        p.value_node_id,
                        now,
                        now
                    ],
                )?;
            }
        }
    }
    Ok(())
}

pub fn list_properties(
    conn: &rusqlite::Connection,
    node_id: &str,
) -> rusqlite::Result<Vec<PropertyInput>> {
    let mut stmt = conn.prepare(
        "SELECT key, value_type, value_text, value_number, value_bool, value_date, value_node_id
         FROM properties WHERE node_id = ?1 ORDER BY key",
    )?;
    let rows = stmt.query_map(params![node_id], |row| {
        Ok(PropertyInput {
            key: row.get(0)?,
            value_type: row.get(1)?,
            value_text: row.get(2)?,
            value_number: row.get(3)?,
            value_bool: row.get(4)?,
            value_date: row.get(5)?,
            value_node_id: row.get(6)?,
        })
    })?;
    rows.collect()
}
