//! Name search over a view tree: Free (letters in order), Word (substring) and Regexp, with
//! the matched char ranges for highlighting.
use regex::{Regex, RegexBuilder};

use super::{Node, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Free,
    Word,
    Regexp,
}

/// Which node text is matched: the lowercase display `name`, or the `label` with its case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Label,
}

pub struct Matcher {
    query: Vec<char>,
    mode: Mode,
    case_sensitive: bool,
    re: Option<Regex>,
}

impl Matcher {
    /// Fails only for a bad regexp.
    pub fn new(query: &str, mode: Mode, case_sensitive: bool) -> Result<Matcher, String> {
        let re = match mode {
            Mode::Regexp => Some(RegexBuilder::new(query).case_insensitive(!case_sensitive).build().map_err(|e| e.to_string())?),
            _ => None,
        };
        Ok(Matcher { query: query.chars().collect(), mode, case_sensitive, re })
    }

    fn same(&self, a: char, b: char) -> bool {
        a == b || (!self.case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
    }

    /// The matched char ranges `[start, end)` in `text`, or `None` when it doesn't match.
    pub fn find(&self, text: &str) -> Option<Vec<(usize, usize)>> {
        if let Some(re) = &self.re {
            let m = re.find(text)?;
            let start = text[..m.start()].chars().count();
            let len = m.as_str().chars().count();
            return Some(if len == 0 { vec![] } else { vec![(start, start + len)] });
        }
        let chars: Vec<char> = text.chars().collect();
        let q = &self.query;
        match self.mode {
            Mode::Word => (0..=chars.len().checked_sub(q.len())?)
                .find(|&i| q.iter().zip(&chars[i..]).all(|(&a, &b)| self.same(a, b)))
                .map(|i| if q.is_empty() { vec![] } else { vec![(i, i + q.len())] }),
            _ => {
                let mut ranges: Vec<(usize, usize)> = Vec::new();
                let mut at = 0;
                for &c in q {
                    let i = at + chars[at..].iter().position(|&t| self.same(c, t))?;
                    match ranges.last_mut() {
                        Some(r) if r.1 == i => r.1 = i + 1,
                        _ => ranges.push((i, i + 1)),
                    }
                    at = i + 1;
                }
                Some(ranges)
            }
        }
    }
}

pub struct Hit {
    pub id: NodeId,
    /// The segments of the hit's parent folder, from the tree root.
    pub parent: Vec<String>,
    /// Char ranges into the node's `name`; empty when they can't be mapped there.
    pub marks: Vec<(usize, usize)>,
}

/// The descendants of `root` (at `root_path`) that match, depth-first in tree order, at most
/// `limit` of them; `true` when more were left out. A node failing `keep` is skipped with
/// everything below it.
pub fn search(
    t: &Tree,
    root: NodeId,
    root_path: &[String],
    m: &Matcher,
    field: Field,
    keep: &dyn Fn(&Node) -> bool,
    limit: usize,
) -> (Vec<Hit>, bool) {
    let mut hits = Vec::new();
    let mut path = root_path.to_vec();
    let truncated = walk(t, root, &mut path, m, field, keep, limit, &mut hits);
    (hits, truncated)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    t: &Tree,
    id: NodeId,
    path: &mut Vec<String>,
    m: &Matcher,
    field: Field,
    keep: &dyn Fn(&Node) -> bool,
    limit: usize,
    hits: &mut Vec<Hit>,
) -> bool {
    for &c in &t.nodes[id].children {
        let n = &t.nodes[c];
        if !keep(n) {
            continue;
        }
        let text = match field {
            Field::Name => &n.name,
            Field::Label => &n.label,
        };
        if let Some(marks) = m.find(text) {
            if hits.len() == limit {
                return true;
            }
            // a label's ranges fit the name when the name is the label lowercased (plus a suffix)
            // and lowercasing kept the char count
            let fits = field == Field::Name || {
                let lower = n.label.to_lowercase();
                n.name.starts_with(&lower) && lower.chars().count() == n.label.chars().count()
            };
            hits.push(Hit { id: c, parent: path.clone(), marks: if fits { marks } else { vec![] } });
        }
        path.push(n.name.clone());
        let full = walk(t, c, path, m, field, keep, limit, hits);
        path.pop();
        if full {
            return true;
        }
    }
    false
}
