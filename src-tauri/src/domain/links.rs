use chrono::Utc;
use rusqlite::{params, Transaction};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::normalize::normalize_title;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkType {
    Wikilink,
    Embed,
}

impl LinkType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            LinkType::Wikilink => "wikilink",
            LinkType::Embed => "embed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LinkStatus {
    Resolved,
    Unresolved,
    Ambiguous,
}

impl LinkStatus {
    fn as_db_str(&self) -> &'static str {
        match self {
            LinkStatus::Resolved => "resolved",
            LinkStatus::Unresolved => "unresolved",
            LinkStatus::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone)]
struct RawLink {
    link_type: LinkType,
    target_raw: String,
    // heading is captured for the byte-range/round-trip but link resolution only
    // targets the note itself — heading-scroll is a Phase 1 UI concern, not a
    // separate resolution target.
    #[allow(dead_code)]
    heading: Option<String>,
    display_text: Option<String>,
    source_start: usize,
    source_end: usize,
}

/// Hand-rolled single-pass tokenizer (not regex) — needed to distinguish
/// `[[Title]]`, `[[Title|Alias]]`, `[[Title#Heading]]`, `![[embed]]`, and to
/// capture byte offsets for re-parsing. Wikilinks don't span lines.
fn tokenize_links(content: &str) -> Vec<RawLink> {
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut links = Vec::new();
    let mut i = 0;

    while i < len {
        let (is_embed, open_start, inner_start) = if bytes[i] == b'!'
            && i + 2 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
        {
            (true, i, i + 3)
        } else if bytes[i] == b'[' && i + 1 < len && bytes[i + 1] == b'[' {
            (false, i, i + 2)
        } else {
            i += 1;
            continue;
        };

        match find_closing(bytes, inner_start) {
            Some(inner_end) => {
                let inner = &content[inner_start..inner_end];
                let (target_and_heading, display_text) = match inner.find('|') {
                    Some(pos) => (&inner[..pos], Some(inner[pos + 1..].to_string())),
                    None => (inner, None),
                };
                let (target_raw, heading) = match target_and_heading.find('#') {
                    Some(pos) => (
                        target_and_heading[..pos].to_string(),
                        Some(target_and_heading[pos + 1..].to_string()),
                    ),
                    None => (target_and_heading.to_string(), None),
                };
                if !target_raw.trim().is_empty() {
                    links.push(RawLink {
                        link_type: if is_embed { LinkType::Embed } else { LinkType::Wikilink },
                        target_raw: target_raw.trim().to_string(),
                        heading: heading.filter(|h| !h.is_empty()),
                        display_text: display_text.filter(|d| !d.is_empty()),
                        source_start: open_start,
                        source_end: inner_end + 2,
                    });
                }
                i = inner_end + 2;
            }
            None => i += 1,
        }
    }
    links
}

/// Returns the byte index of the first `]` of the closing `]]`, or `None` if
/// unterminated or a newline is hit first (wikilinks are single-line).
fn find_closing(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start;
    while j + 1 < bytes.len() {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            return Some(j);
        }
        if bytes[j] == b'\n' {
            return None;
        }
        j += 1;
    }
    None
}

enum ResolveResult {
    One(String),
    None,
    Ambiguous,
}

/// Resolves a normalized target against `nodes.title_normalized` first (more
/// than one match — titles aren't unique — is `Ambiguous`), then `aliases`.
fn resolve_target(txn: &Transaction, normalized: &str) -> rusqlite::Result<ResolveResult> {
    let mut stmt =
        txn.prepare("SELECT id FROM nodes WHERE title_normalized = ?1 AND deleted_at IS NULL")?;
    let ids: Vec<String> = stmt
        .query_map(params![normalized], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    match ids.len() {
        1 => return Ok(ResolveResult::One(ids.into_iter().next().unwrap())),
        n if n > 1 => return Ok(ResolveResult::Ambiguous),
        _ => {}
    }

    let alias: Option<String> = txn
        .query_row(
            "SELECT node_id FROM aliases WHERE normalized_alias = ?1",
            params![normalized],
            |row| row.get(0),
        )
        .ok();
    Ok(match alias {
        Some(id) => ResolveResult::One(id),
        None => ResolveResult::None,
    })
}

/// Re-tokenizes `content` and replaces `source_node_id`'s outgoing links wholesale
/// (delete + reinsert, inside the caller's transaction). Link rows carry no
/// independent mutable state worth diffing, so this stays correct and simple at
/// note-taking scale — see the plan for why incremental diffing isn't worth it.
pub fn reparse_links(txn: &Transaction, source_node_id: &str, content: &str) -> rusqlite::Result<()> {
    txn.execute(
        "DELETE FROM links WHERE source_node_id = ?1",
        params![source_node_id],
    )?;

    let now = Utc::now().to_rfc3339();
    for link in tokenize_links(content) {
        let normalized = normalize_title(&link.target_raw);
        let resolved = resolve_target(txn, &normalized)?;
        let (target_node_id, status) = match resolved {
            ResolveResult::One(id) => (Some(id), LinkStatus::Resolved),
            ResolveResult::None => (None, LinkStatus::Unresolved),
            ResolveResult::Ambiguous => (None, LinkStatus::Ambiguous),
        };
        let id = Uuid::new_v4().to_string();
        txn.execute(
            "INSERT INTO links (
                id, source_node_id, target_node_id, target_raw, display_text,
                link_type, status, source_start, source_end, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                source_node_id,
                target_node_id,
                link.target_raw,
                link.display_text,
                link.link_type.as_db_str(),
                status.as_db_str(),
                link.source_start as i64,
                link.source_end as i64,
                now,
            ],
        )?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct OutgoingLinkDto {
    pub id: String,
    pub target_node_id: Option<String>,
    pub target_raw: String,
    pub display_text: Option<String>,
    pub link_type: String,
    pub status: String,
}

pub fn list_outgoing_links(
    conn: &rusqlite::Connection,
    node_id: &str,
) -> rusqlite::Result<Vec<OutgoingLinkDto>> {
    let mut stmt = conn.prepare(
        "SELECT id, target_node_id, target_raw, display_text, link_type, status
         FROM links WHERE source_node_id = ?1 ORDER BY source_start",
    )?;
    let rows = stmt.query_map(params![node_id], |row| {
        Ok(OutgoingLinkDto {
            id: row.get(0)?,
            target_node_id: row.get(1)?,
            target_raw: row.get(2)?,
            display_text: row.get(3)?,
            link_type: row.get(4)?,
            status: row.get(5)?,
        })
    })?;
    rows.collect()
}
