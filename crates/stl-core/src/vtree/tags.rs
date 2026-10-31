//! Tags: the model, generated variant tags, user overrides and the `tag:` search syntax.
use std::collections::HashMap;

use super::{Node, Tree};

/// The tag kinds; the derived order is the dot order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TagKind {
    User,
    Support,
    Scale,
    Creator,
    Edition,
}

impl TagKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TagKind::User => "user",
            TagKind::Support => "support",
            TagKind::Scale => "scale",
            TagKind::Creator => "creator",
            TagKind::Edition => "edition",
        }
    }

    pub fn parse(s: &str) -> Option<TagKind> {
        Some(match s {
            "user" => TagKind::User,
            "support" => TagKind::Support,
            "scale" => TagKind::Scale,
            "creator" => TagKind::Creator,
            "edition" => TagKind::Edition,
            _ => return None,
        })
    }

    /// Editions are generated only.
    pub fn settable(self) -> bool {
        self != TagKind::Edition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag {
    pub kind: TagKind,
    pub name: String,
    pub by_user: bool,
}

/// Trim, lowercase and collapse whitespace runs; `None` for an empty name.
pub fn normalize(name: &str) -> Option<String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    (!n.is_empty()).then_some(n)
}

/// The user-data key of a real path: `r:` + lowercase, `/` as `\`, no trailing `\`.
pub fn tag_key(real: &str) -> String {
    format!("r:{}", real.replace('/', "\\").trim_end_matches('\\').to_lowercase())
}

/// One key per real path of the node, deduped.
pub fn tag_keys(n: &Node) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for r in &n.real {
        let k = tag_key(r);
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys
}

/// Generated tags of a variant label such as `supported+25mm`.
pub fn variant_tags(label: &str) -> Vec<Tag> {
    let mk = |kind, name: &str| Tag { kind, name: name.to_string(), by_user: false };
    label
        .split('+')
        .filter_map(|atom| {
            let atom = atom.trim().to_lowercase();
            if matches!(atom.as_str(), "supported" | "unsupported" | "lychee" | "chitubox") {
                Some(mk(TagKind::Support, &atom))
            } else if atom.strip_suffix("mm").is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())) {
                Some(mk(TagKind::Scale, &atom))
            } else {
                None
            }
        })
        .collect()
}

/// Apply the exclusivity rules, then sort and dedupe: supported and unsupported cancel each
/// other, and more than one distinct scale gives no scale.
pub fn exclusive(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.sort();
    tags.dedup();
    let has = |tags: &[Tag], kind, name: &str| tags.iter().any(|t| t.kind == kind && t.name == name);
    if has(&tags, TagKind::Support, "supported") && has(&tags, TagKind::Support, "unsupported") {
        tags.retain(|t| !(t.kind == TagKind::Support && matches!(t.name.as_str(), "supported" | "unsupported")));
    }
    if tags.iter().filter(|t| t.kind == TagKind::Scale).count() > 1 {
        tags.retain(|t| t.kind != TagKind::Scale);
    }
    tags
}

/// The tags every one of `sets` has; empty when there are no sets.
fn common(sets: &[&Vec<Tag>]) -> Vec<Tag> {
    let Some((first, rest)) = sets.split_first() else { return Vec::new() };
    first.iter().filter(|t| rest.iter().all(|s| s.contains(t))).cloned().collect()
}

/// Give each Files node without generated tags the tags its Models counterparts have in common:
/// leaves match on their `f:` key first, then every node matches on its real paths.
pub fn copy_tags(models: &Tree, files: &mut Tree) {
    let mut by_key: HashMap<&str, Vec<&Vec<Tag>>> = HashMap::new();
    let mut by_real: HashMap<String, Vec<&Vec<Tag>>> = HashMap::new();
    for n in models.nodes.iter().skip(1) {
        if n.key.starts_with("f:") {
            by_key.entry(n.key.as_str()).or_default().push(&n.info.tags);
        }
        for k in tag_keys(n) {
            by_real.entry(k).or_default().push(&n.info.tags);
        }
    }
    for n in files.nodes.iter_mut().skip(1) {
        if !n.info.tags.is_empty() {
            continue;
        }
        if let Some(sets) = by_key.get(n.key.as_str()) {
            n.info.tags = common(sets);
            continue;
        }
        let mut sets: Vec<&Vec<Tag>> = Vec::new();
        for k in tag_keys(n) {
            if let Some(found) = by_real.get(&k) {
                sets.extend(found.iter().copied());
            }
        }
        // a node reached through several of its paths counts once per path; the intersection is unchanged
        n.info.tags = common(&sets);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    /// The name text: every term that is not a `tag:` term, joined by single spaces.
    pub text: String,
    /// Normalised tag names.
    pub tags: Vec<String>,
}

/// Split `tag:x` / `tag:"x y"` terms from the name text.
pub fn parse_query(q: &str) -> ParsedQuery {
    let mut text: Vec<&str> = Vec::new();
    let mut tags = Vec::new();
    let mut rest = q;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if rest.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("tag:")) {
            let after = &rest[4..];
            let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
                quoted.split_once('"').unwrap_or((quoted, ""))
            } else {
                after.split_once(char::is_whitespace).unwrap_or((after, ""))
            };
            if let Some(n) = normalize(value) {
                tags.push(n);
            }
            rest = tail;
        } else {
            let (term, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            text.push(term);
            rest = tail;
        }
    }
    ParsedQuery { text: text.join(" "), tags }
}

/// The user's tag edits, keyed by `tag_key`.
#[derive(Debug, Clone, Default)]
pub struct UserTags {
    /// Tag key -> user-set tags (`by_user = true`).
    pub set: HashMap<String, Vec<Tag>>,
    /// Tag key -> generated tags the user hid.
    pub hidden: HashMap<String, Vec<(TagKind, String)>>,
}

impl UserTags {
    /// The node's own tags: generated ones minus hidden and minus kinds the user overrode,
    /// plus the user-set ones, sorted by (kind, name).
    pub fn effective(&self, n: &Node) -> Vec<Tag> {
        let keys = tag_keys(n);
        let user: Vec<&Tag> = keys.iter().filter_map(|k| self.set.get(k)).flatten().collect();
        let hidden = self.hidden_pairs(&keys);
        let mut out: Vec<Tag> = n
            .info
            .tags
            .iter()
            .filter(|t| !hidden.iter().any(|(k, name)| *k == t.kind && *name == t.name))
            .filter(|t| !user.iter().any(|u| u.kind == t.kind && u.kind != TagKind::User))
            .cloned()
            .collect();
        out.extend(user.into_iter().cloned());
        out.sort();
        out.dedup();
        out
    }

    /// The node's generated tags that the user hid.
    pub fn hidden_of(&self, n: &Node) -> Vec<Tag> {
        let hidden = self.hidden_pairs(&tag_keys(n));
        let mut out: Vec<Tag> = n
            .info
            .tags
            .iter()
            .filter(|t| hidden.iter().any(|(k, name)| *k == t.kind && *name == t.name))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn hidden_pairs(&self, keys: &[String]) -> Vec<&(TagKind, String)> {
        keys.iter().filter_map(|k| self.hidden.get(k)).flatten().collect()
    }
}
