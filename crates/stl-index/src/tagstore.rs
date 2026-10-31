//! User tag edits: user-set tags (`tags` + `node_tags`) and hidden generated tags (`hidden_tags`),
//! keyed by tag key. All of it is user data, so a re-index never touches it.
use rusqlite::{Connection, params};
use stl_core::vtree::tags::{Tag, TagKind, UserTags, normalize};

use crate::scanstore::IndexError;

fn parse_kind(s: &str) -> Option<TagKind> {
    TagKind::parse(s)
}

/// Every user-set and hidden tag, with `by_user` set on the former.
pub fn load(conn: &Connection) -> Result<UserTags, IndexError> {
    let mut out = UserTags::default();
    let mut st = conn.prepare(
        "SELECT nt.tag_key, t.kind, t.name FROM node_tags nt JOIN tags t ON t.id = nt.tag_id ORDER BY nt.tag_key, t.kind, t.name",
    )?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    for row in rows {
        let (key, kind, name) = row?;
        if let Some(kind) = parse_kind(&kind) {
            out.set.entry(key).or_default().push(Tag { kind, name, by_user: true });
        }
    }
    let mut st = conn.prepare("SELECT tag_key, kind, name FROM hidden_tags ORDER BY tag_key, kind, name")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    for row in rows {
        let (key, kind, name) = row?;
        if let Some(kind) = parse_kind(&kind) {
            out.hidden.entry(key).or_default().push((kind, name));
        }
    }
    for tags in out.set.values_mut() {
        tags.sort();
    }
    Ok(out)
}

fn clean(kind: TagKind, name: &str) -> Result<String, IndexError> {
    if !kind.settable() {
        return Err(IndexError::Invalid(format!("{} tags can't be set", kind.as_str())));
    }
    normalize(name).ok_or_else(|| IndexError::Invalid("a tag name can't be empty".into()))
}

/// Delete `tags` rows no key uses any more.
fn prune(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM node_tags)", [])?;
    Ok(())
}

/// Tag every key with `name`. A non-`User` kind replaces the kind's other user-set values.
pub fn add(conn: &Connection, keys: &[String], kind: TagKind, name: &str) -> Result<(), IndexError> {
    let name = clean(kind, name)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT OR IGNORE INTO tags(kind, name) VALUES (?1, ?2)", params![kind.as_str(), name])?;
    let id: i64 = tx.query_row("SELECT id FROM tags WHERE kind = ?1 AND name = ?2", params![kind.as_str(), name], |r| r.get(0))?;
    for key in keys {
        if kind != TagKind::User {
            tx.execute(
                "DELETE FROM node_tags WHERE tag_key = ?1 AND tag_id != ?2 AND tag_id IN (SELECT id FROM tags WHERE kind = ?3)",
                params![key, id, kind.as_str()],
            )?;
        }
        tx.execute("INSERT OR IGNORE INTO node_tags(tag_key, tag_id) VALUES (?1, ?2)", params![key, id])?;
    }
    prune(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Remove a user-set tag from every key; names left unused are deleted.
pub fn remove(conn: &Connection, keys: &[String], kind: TagKind, name: &str) -> Result<(), IndexError> {
    let Some(name) = normalize(name) else { return Ok(()) };
    let tx = conn.unchecked_transaction()?;
    for key in keys {
        tx.execute(
            "DELETE FROM node_tags WHERE tag_key = ?1 AND tag_id IN (SELECT id FROM tags WHERE kind = ?2 AND name = ?3)",
            params![key, kind.as_str(), name],
        )?;
    }
    prune(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Hide a generated tag on every key.
pub fn hide(conn: &Connection, keys: &[String], kind: TagKind, name: &str) -> Result<(), IndexError> {
    let Some(name) = normalize(name) else {
        return Err(IndexError::Invalid("a tag name can't be empty".into()));
    };
    let tx = conn.unchecked_transaction()?;
    for key in keys {
        tx.execute(
            "INSERT OR IGNORE INTO hidden_tags(tag_key, kind, name) VALUES (?1, ?2, ?3)",
            params![key, kind.as_str(), name],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Show a hidden generated tag again on every key.
pub fn unhide(conn: &Connection, keys: &[String], kind: TagKind, name: &str) -> Result<(), IndexError> {
    let Some(name) = normalize(name) else { return Ok(()) };
    let tx = conn.unchecked_transaction()?;
    for key in keys {
        tx.execute(
            "DELETE FROM hidden_tags WHERE tag_key = ?1 AND kind = ?2 AND name = ?3",
            params![key, kind.as_str(), name],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// The user-set names in use, sorted by (kind, name).
pub fn names(conn: &Connection) -> Result<Vec<(TagKind, String)>, IndexError> {
    let mut st = conn.prepare("SELECT DISTINCT t.kind, t.name FROM tags t JOIN node_tags nt ON nt.tag_id = t.id")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, name) = row?;
        if let Some(kind) = parse_kind(&kind) {
            out.push((kind, name));
        }
    }
    out.sort();
    Ok(out)
}
