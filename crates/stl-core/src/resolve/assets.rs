//! Attach images and docs to models and groups, adopt catalog ids from pictures, and find unit
//! groups (P18). Port of the asset half of `prototype/stlindex/resolve.py`.
use std::collections::{HashMap, HashSet};

use super::detect::Pass;
use super::tree::NodeId;
use super::{Asset, AssetRole, Match};
use crate::names::{self, ImageRole, Kind};

fn role_order(r: AssetRole) -> u8 {
    match r {
        AssetRole::Preview => 0,
        AssetRole::Thumb => 1,
        AssetRole::Instructions => 2,
        AssetRole::Doc => 3,
        AssetRole::Poster => 9,
    }
}

fn subtree_models(p: &Pass, node: NodeId) -> Vec<usize> {
    let mut out = Vec::new();
    for n in p.tree.walk(node) {
        if let Some(m) = p.model_of[n]
            && !out.contains(&m)
        {
            out.push(m);
        }
    }
    out
}

fn is_below(p: &Pass, node: NodeId, ancestor: NodeId) -> bool {
    let mut n = Some(node);
    while let Some(x) = n {
        if x == ancestor {
            return true;
        }
        n = p.tree.nodes[x].parent;
    }
    false
}

fn nearest_group(p: &Pass, node: NodeId) -> Option<usize> {
    let mut n = Some(node);
    while let Some(x) = n {
        if let Some(g) = p.group_of[x] {
            return Some(g);
        }
        if let Some(m) = p.model_of[x]
            && let Some(g) = p.models[m].group
        {
            return Some(g);
        }
        n = p.tree.nodes[x].parent;
    }
    None
}

fn match_by_id(fname: &str, cands: &[usize], p: &Pass) -> Option<usize> {
    let toks = names::tokens(names::stem(fname));
    let ids: HashSet<String> = toks
        .iter()
        .filter(|t| names::is_digits(t) && t.chars().count() >= 3)
        .cloned()
        .chain(toks.iter().filter(|t| t.to_lowercase().starts_with("hl")).map(|t| t.to_uppercase()))
        .collect();
    let hits: Vec<usize> =
        cands.iter().copied().filter(|&m| p.models[m].attrs.catalog_id.as_ref().is_some_and(|c| ids.contains(c))).collect();
    if hits.len() == 1 { Some(hits[0]) } else { None }
}

fn name_tokens(name: &str) -> Vec<String> {
    names::tokens(name).into_iter().filter(|t| t.chars().count() >= 3 && !names::is_digits(t)).map(|t| names::singular(&t)).collect()
}

/// Index into `cands` of the one candidate name the image is named after. `context`: tokens of
/// the folder the image sits in (and its ancestors); a candidate token that just repeats them
/// ("BUILDER 1" under "RENEGADE TROOPER BUILDER") is no evidence.
fn match_by_name(fname: &str, cands: &[&str], context: &HashSet<String>) -> Option<usize> {
    let raw = names::image_tokens(fname);
    let itoks: HashSet<String> = raw.iter().map(|t| names::singular(t)).collect();
    if itoks.is_empty() {
        return None;
    }
    // split compounds: "ws-exo-suits-render" should hit the model word "Exosuits"
    let mut hit_toks = itoks.clone();
    hit_toks.extend(raw.windows(2).map(|w| names::singular(&format!("{}{}", w[0], w[1]))));
    // tokens shared by most candidates ("War Sisters - ...") don't identify a model
    let mut counts: HashMap<String, usize> = HashMap::new();
    for c in cands {
        for t in name_tokens(c).into_iter().collect::<HashSet<_>>() {
            *counts.entry(t).or_default() += 1;
        }
    }
    let n = cands.len();
    let common: HashSet<String> =
        counts.into_iter().filter(|(_, c)| n >= 3 && *c as f64 > n as f64 / 2.0).map(|(t, _)| t).collect();
    // the image's own identifying words ("WS Bases 1" -> {base})
    let idistinct: HashSet<&String> =
        itoks.iter().filter(|t| t.chars().count() >= 3 && !names::is_digits(t) && !context.contains(*t)).collect();
    let mut scored: Vec<(f64, usize, usize)> = Vec::new();
    for (i, c) in cands.iter().enumerate() {
        let own: Vec<String> = name_tokens(c).into_iter().filter(|t| !context.contains(t)).collect();
        let distinct: Vec<String> = own.iter().filter(|t| !common.contains(*t)).cloned().collect();
        let mtoks = if distinct.is_empty() { own.clone() } else { distinct };
        if mtoks.is_empty() {
            continue;
        }
        let hit = mtoks.iter().filter(|t| hit_toks.contains(*t)).count();
        let ratio = hit as f64 / mtoks.len() as f64;
        let own_set: HashSet<&String> = own.iter().collect();
        if hit == mtoks.len()
            || (hit >= 2 && ratio >= 0.6)
            || (hit > 0 && !idistinct.is_empty() && idistinct.iter().all(|t| own_set.contains(t)))
        {
            scored.push((ratio, hit, i));
        }
    }
    // stable sort, best first by (hit, ratio)
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.partial_cmp(&a.0).expect("finite ratio")));
    match scored.as_slice() {
        [] => None,
        [a, b, ..] if a.0 == b.0 && a.1 == b.1 => None,
        [a, ..] => Some(a.2),
    }
}

fn context_tokens(p: &Pass, node: NodeId) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut n = Some(node);
    while let Some(x) = n {
        out.extend(name_tokens(&p.tree.nodes[x].name));
        n = p.tree.nodes[x].parent;
    }
    out
}

/// D1/D4: a picture next to an archive or folder of (nearly) the same name belongs to it:
/// `Eternal Falcon (3).jpg` / `Eternal Falcon.rar`, `SF_Empress_fina.jpg` / `SF_Empress_final.rar`.
fn match_sidecar(p: &Pass, fname: &str, node: NodeId) -> Option<NodeId> {
    let key = names::sidecar_key(fname);
    let klen = key.chars().count();
    if klen < 4 {
        return None;
    }
    let mut scored: Vec<(f64, NodeId)> = Vec::new();
    for &c in p.tree.nodes[node].children.values() {
        if !p.tree.nodes[c].structural {
            continue;
        }
        let ck = names::sidecar_key(&p.tree.nodes[c].name);
        let shorter = ck.chars().count().min(klen);
        if ck == key {
            scored.push((3.0, c));
        } else if shorter >= 6 && (ck.starts_with(&key) || key.starts_with(&ck)) {
            scored.push((1.0 + shorter as f64 / 1000.0, c));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).expect("finite score"));
    match scored.as_slice() {
        [] => None,
        [a, b, ..] if a.0 == b.0 => None,
        [a, ..] => Some(a.1),
    }
}

fn first_group_at_or_below(p: &Pass, node: NodeId) -> Option<usize> {
    p.tree.walk(node).into_iter().find_map(|n| p.group_of[n])
}

/// A model folder without a number ("Hogriderz 1") whose own pictures carry one
/// ("442 Complete hogriderz 1.png") takes that number, so group-level images can match by id.
fn adopt_catalog_ids(p: &mut Pass, assets: &[Asset]) {
    for m in p.models.iter_mut() {
        if m.attrs.catalog_id.is_some() {
            continue;
        }
        let ids: HashSet<String> = m
            .assets
            .iter()
            .map(|&a| &assets[a])
            .filter(|a| a.kind == Kind::Image)
            .filter_map(|a| {
                let n = names::normalize(&a.name);
                names::catalog_id(n.strip_prefix("resize ").unwrap_or(&n))
            })
            .filter(|i| !(names::is_digits(i) && i.parse::<u32>().is_ok_and(|y| (1990..=2035).contains(&y)))) // not a year
            .collect();
        if ids.len() == 1 {
            m.attrs.catalog_id = ids.into_iter().next();
        }
    }
}

pub(crate) fn attach_assets(p: &mut Pass) -> Vec<Asset> {
    let mut assets: Vec<Asset> = Vec::new();
    let mut pending: Vec<(usize, NodeId)> = Vec::new();
    let mut by_part: HashMap<String, usize> = HashMap::new();
    for (mi, m) in p.models.iter().enumerate() {
        for f in &m.files {
            by_part.entry(f.name.to_lowercase()).or_insert(mi);
        }
    }
    // pass 1: thumbnails and images that sit inside a model
    for node in p.tree.walk(0) {
        for f in &p.tree.nodes[node].files {
            if !matches!(f.kind, Kind::Image | Kind::Doc) {
                continue;
            }
            let role = if f.kind == Kind::Image {
                match names::image_role(&f.name) {
                    ImageRole::Preview => AssetRole::Preview,
                    ImageRole::Thumb => AssetRole::Thumb,
                    ImageRole::Instructions => AssetRole::Instructions,
                }
            } else {
                AssetRole::Doc
            };
            let e = &p.entries[f.entry];
            let mut a = Asset {
                entry_id: e.id,
                entry_size: e.size,
                name: f.name.clone(),
                kind: f.kind,
                role,
                match_: Match::Poster,
                model: None,
                group: None,
                generic: f.kind == Kind::Image && names::is_generic_image(&f.name),
                depth: 0,
            };
            let ai = assets.len();
            let thumb_of = by_part.get(&names::stem(&f.name).to_lowercase()).copied();
            if let (AssetRole::Thumb, Some(m)) = (role, thumb_of) {
                a.model = Some(m);
                a.match_ = Match::Thumb;
            } else if let Some(m) = p.model_of[node] {
                a.model = Some(m);
                a.match_ = Match::Inside;
                let top = p.model_node[m];
                let (mut x, mut d) = (Some(node), 0);
                while let Some(n) = x {
                    if n == top {
                        break;
                    }
                    x = p.tree.nodes[n].parent;
                    d += 1;
                }
                a.depth = d;
            } else {
                assets.push(a);
                pending.push((ai, node));
                continue;
            }
            let m = a.model.expect("set above");
            assets.push(a);
            p.models[m].assets.push(ai);
        }
    }
    adopt_catalog_ids(p, &assets);
    // pass 2: images above models -> sidecar, only child, catalog id, name; else poster of best group
    for (ai, node) in pending {
        let (name, is_image) = (assets[ai].name.clone(), assets[ai].kind == Kind::Image);
        if is_image && let Some(side) = match_sidecar(p, &name, node) {
            let ms = subtree_models(p, side);
            if ms.len() == 1 {
                let a = &mut assets[ai];
                (a.model, a.depth, a.match_) = (Some(ms[0]), 0, Match::Sidecar);
                p.models[ms[0]].assets.push(ai);
                continue;
            }
            if let Some(g) = first_group_at_or_below(p, side) {
                let a = &mut assets[ai];
                (a.group, a.match_) = (Some(g), Match::Sidecar);
                if a.role == AssetRole::Preview {
                    a.role = AssetRole::Poster;
                }
                continue;
            }
        }
        let cands = subtree_models(p, node);
        let context = context_tokens(p, node);
        let (hit, how) = if cands.len() == 1 {
            (Some(cands[0]), Match::Only)
        } else {
            match match_by_id(&name, &cands, p) {
                Some(m) => (Some(m), Match::Id),
                None => {
                    let cnames: Vec<&str> = cands.iter().map(|&m| p.models[m].name.as_str()).collect();
                    (match_by_name(&name, &cnames, &context).map(|i| cands[i]), Match::Name)
                }
            }
        };
        let a = &mut assets[ai];
        if let Some(m) = hit {
            (a.model, a.depth, a.match_) = (Some(m), 1, how);
            p.models[m].assets.push(ai);
            continue;
        }
        let sub: Vec<usize> = (0..p.groups.len())
            .filter(|&g| p.group_node[g] != node && is_below(p, p.group_node[g], node))
            .collect();
        let snames: Vec<&str> = sub.iter().map(|&g| p.groups[g].name.as_str()).collect();
        a.group = match_by_name(&name, &snames, &context).map(|i| sub[i]).or_else(|| nearest_group(p, node));
        a.match_ = Match::Poster;
        if a.role == AssetRole::Preview {
            a.role = AssetRole::Poster;
        }
    }
    for m in p.models.iter_mut() {
        m.assets.sort_by_key(|&i| {
            let a = &assets[i];
            (
                a.kind != Kind::Image,
                role_order(a.role),
                a.generic,
                a.depth,
                if a.generic { -(a.entry_size as i64) } else { 0 },
                a.name.to_lowercase(),
            )
        });
    }
    assets
}

/// Groups whose models have no images of their own while the group has some: the images
/// describe the group as a whole (a squad / kit / battalion), so it should be one model.
/// Top-down: once a group is chosen, its subgroups are not considered separately.
pub(crate) fn unit_groups(p: &Pass, assets: &[Asset]) -> Vec<NodeId> {
    let imaged: HashSet<usize> =
        (0..p.models.len()).filter(|&m| p.models[m].assets.iter().any(|&a| assets[a].kind == Kind::Image)).collect();
    let posters: HashSet<usize> =
        assets.iter().filter(|a| a.model.is_none() && a.kind == Kind::Image).filter_map(|a| a.group).collect();
    let under = |group: Option<usize>, ancestor: usize| -> bool {
        let mut g = group;
        while let Some(x) = g {
            if x == ancestor {
                return true;
            }
            g = p.groups[x].parent;
        }
        false
    };
    let mut chosen: Vec<usize> = Vec::new();
    for g in 0..p.groups.len() {
        // parents come before children
        let mut q = p.groups[g].parent;
        while let Some(x) = q {
            if chosen.contains(&x) {
                break;
            }
            q = p.groups[x].parent;
        }
        if q.is_some() || !posters.contains(&g) {
            continue;
        }
        let inside: Vec<usize> = (0..p.models.len()).filter(|&m| under(p.models[m].group, g)).collect();
        if !inside.is_empty() && !inside.iter().any(|m| imaged.contains(m)) {
            chosen.push(g);
        }
    }
    chosen.into_iter().map(|g| p.group_node[g]).collect()
}
