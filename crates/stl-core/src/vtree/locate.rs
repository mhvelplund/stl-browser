//! Find the place in one view that matches a folder of the other, by their real paths, so that
//! switching views keeps the user near where they were.
use super::{NodeId, Tree, is_file_kind};

/// A real path's components, lowercase: split at `\`, `/` and the `!` of archive boundaries.
fn parts(real: &str) -> Vec<String> {
    real.split(['\\', '/', '!']).filter(|p| !p.is_empty()).map(str::to_lowercase).collect()
}

/// The folder of `to` that best matches the folder at `path` in `from`, as a name path:
/// - the deepest folder whose real path contains this one's (equal counts);
/// - else the shallowest folder below it;
/// - else the same for the parent, one level up at a time, ending at the root (`[]`).
pub fn locate<S: AsRef<str>>(from: &Tree, path: &[S], to: &Tree) -> Vec<String> {
    let (_, used) = from.walk(path);
    let names: Vec<&str> = path[..used].iter().map(AsRef::as_ref).collect();
    for len in (1..=names.len()).rev() {
        let (id, _) = from.walk(&names[..len]);
        let wanted: Vec<Vec<String>> = from.nodes[id].real.iter().map(|r| parts(r)).collect();
        if let Some(found) = best(to, &wanted) {
            return found;
        }
    }
    Vec::new()
}

/// Depth-first over the folders of `to`, carrying each one's name path.
fn best(to: &Tree, wanted: &[Vec<String>]) -> Option<Vec<String>> {
    if wanted.is_empty() {
        return None;
    }
    // (matched components, tree depth) — more is better for a container
    let mut container: Option<((usize, usize), NodeId)> = None;
    // (matched components, tree depth) — less is better for a folder below
    let mut below: Option<((usize, usize), NodeId)> = None;
    let mut path_of: Vec<Option<(NodeId, usize)>> = vec![None; to.nodes.len()]; // parent, depth
    let mut stack: Vec<NodeId> = vec![0];
    while let Some(id) = stack.pop() {
        let depth = path_of[id].map_or(0, |(_, d)| d);
        for &c in to.nodes[id].children.iter().rev() {
            if !is_file_kind(to.nodes[c].kind) {
                path_of[c] = Some((id, depth + 1));
                stack.push(c);
            }
        }
        for real in &to.nodes[id].real {
            let have = parts(real);
            for w in wanted {
                let score = (have.len(), depth);
                if w.starts_with(&have) {
                    if container.is_none_or(|(s, _)| score > s) {
                        container = Some((score, id));
                    }
                } else if have.starts_with(w) && below.is_none_or(|(s, _)| score < s) {
                    below = Some((score, id));
                }
            }
        }
    }
    let (_, id) = container.or(below)?;
    let mut names = Vec::new();
    let mut at = id;
    while let Some((parent, _)) = path_of[at] {
        names.push(to.nodes[at].name.clone());
        at = parent;
    }
    names.reverse();
    Some(names)
}
