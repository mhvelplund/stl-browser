//! Name search over a view tree: Free (letters in order), Word (substring) and Regexp, with
//! the matched char ranges for highlighting.
use std::collections::HashSet;

use regex::{Regex, RegexBuilder};

use super::tags::{Tag, TagKind};
use super::{is_file_kind, Node, NodeId, Tree};

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

/// The tags a node inherits from its ancestors, and the kinds a user-set tag overrode (which
/// mask generated tags of that kind below).
#[derive(Debug, Clone, Default)]
pub struct Inherited {
    tags: HashSet<Tag>,
    overridden: HashSet<TagKind>,
}

impl Inherited {
    fn visible<'a>(&'a self, own: &'a [Tag]) -> impl Iterator<Item = &'a Tag> {
        own.iter().filter(|t| t.by_user || !self.overridden.contains(&t.kind))
    }

    /// What a child of a node with these own (effective) tags inherits. A kind the node sets by
    /// user also drops the generated tags of that kind it inherited.
    pub fn push(&self, own: &[Tag]) -> Inherited {
        let mut next = self.clone();
        let set: Vec<TagKind> = own.iter().filter(|t| t.by_user && t.kind != TagKind::User).map(|t| t.kind).collect();
        next.tags.retain(|t| t.by_user || !set.contains(&t.kind));
        next.tags.extend(self.visible(own).cloned());
        next.overridden.extend(set);
        next
    }

    /// Whether `own` plus what was inherited covers every term.
    pub fn covers(&self, own: &[Tag], terms: &[String]) -> bool {
        terms.iter().all(|term| self.tags.iter().chain(self.visible(own)).any(|t| &t.name == term))
    }

    /// Inherited set at the folder `path` (the root to that folder, the folder's own tags included).
    pub fn along(t: &Tree, path: &[String], own: &dyn Fn(NodeId) -> Vec<Tag>) -> Inherited {
        let mut inh = Inherited::default();
        let mut node = 0;
        for part in path {
            let part = part.to_lowercase();
            match t.nodes[node].children.iter().find(|&&c| t.nodes[c].name == part && !is_file_kind(t.nodes[c].kind)) {
                Some(&c) => {
                    inh = inh.push(&own(c));
                    node = c;
                }
                None => break,
            }
        }
        inh
    }
}

/// The tag terms of a search and how to read a node's own tags.
pub struct TagFilter<'a> {
    pub terms: &'a [String],
    pub own: &'a dyn Fn(NodeId) -> Vec<Tag>,
    /// What the search root already inherits.
    pub start: Inherited,
}

struct Ctx<'a> {
    m: Option<&'a Matcher>,
    field: Field,
    keep: &'a dyn Fn(&Node) -> bool,
    tags: &'a TagFilter<'a>,
    limit: usize,
}

/// The descendants of `root` (at `root_path`) that match, depth-first in tree order, at most
/// `limit` of them; `true` when more were left out. A node failing `keep` is skipped with
/// everything below it. A hit needs a name match (when `m` is given) and every tag term; with
/// no matcher, a hit is the topmost passing node and the walk doesn't descend into it, and
/// with neither matcher nor terms there are no hits.
#[allow(clippy::too_many_arguments)]
pub fn search(
    t: &Tree,
    root: NodeId,
    root_path: &[String],
    m: Option<&Matcher>,
    field: Field,
    keep: &dyn Fn(&Node) -> bool,
    tags: &TagFilter,
    limit: usize,
) -> (Vec<Hit>, bool) {
    let mut hits = Vec::new();
    if m.is_none() && tags.terms.is_empty() {
        return (hits, false);
    }
    let mut path = root_path.to_vec();
    let ctx = Ctx { m, field, keep, tags, limit };
    let truncated = walk(t, root, &mut path, &ctx, &tags.start, &mut hits);
    (hits, truncated)
}

fn walk(t: &Tree, id: NodeId, path: &mut Vec<String>, cx: &Ctx, inh: &Inherited, hits: &mut Vec<Hit>) -> bool {
    for &c in &t.nodes[id].children {
        let n = &t.nodes[c];
        if !(cx.keep)(n) {
            continue;
        }
        // without tag terms nothing reads the tags, so skip the (costly) lookup
        let by_tags = !cx.tags.terms.is_empty();
        let own = if by_tags { (cx.tags.own)(c) } else { Vec::new() };
        let covered = !by_tags || inh.covers(&own, cx.tags.terms);
        let mut marks = None;
        match cx.m {
            Some(m) => {
                let text = match cx.field {
                    Field::Name => &n.name,
                    Field::Label => &n.label,
                };
                if covered {
                    marks = m.find(text);
                }
            }
            None => marks = covered.then(Vec::new),
        }
        if let Some(marks) = marks {
            if hits.len() == cx.limit {
                return true;
            }
            // a label's ranges fit the name when the name is the label lowercased (plus a suffix)
            // and lowercasing kept the char count
            let fits = cx.field == Field::Name || {
                let lower = n.label.to_lowercase();
                n.name.starts_with(&lower) && lower.chars().count() == n.label.chars().count()
            };
            hits.push(Hit { id: c, parent: path.clone(), marks: if fits { marks } else { vec![] } });
            if cx.m.is_none() {
                continue;
            }
        }
        if n.children.is_empty() {
            continue;
        }
        path.push(n.name.clone());
        let full = if by_tags { walk(t, c, path, cx, &inh.push(&own), hits) } else { walk(t, c, path, cx, inh, hits) };
        path.pop();
        if full {
            return true;
        }
    }
    false
}
