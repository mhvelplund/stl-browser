//! Name heuristics: file kinds and name normalisation. Port of `prototype/stlindex/names.py`.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

pub const MODEL_EXTS: &[&str] = &[".stl", ".obj", ".3mf", ".ply", ".fbx", ".blend", ".ztl", ".zpr", ".step", ".stp"];
pub const SCENE_EXTS: &[&str] = &[".lys", ".lyt", ".chitubox", ".ctb", ".cbddlp", ".goo", ".photon", ".pwmo", ".pwms", ".sl1"];
pub const IMAGE_EXTS: &[&str] = &[".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];
pub const COMPANION_EXTS: &[&str] = &[".mtl"];
pub const DOC_EXTS: &[&str] = &[".pdf", ".txt", ".md", ".html", ".htm", ".rtf"];
pub const ARCHIVE_EXTS: &[&str] = &[".zip", ".rar", ".7z"];
pub const JUNK_NAMES: &[&str] = &["desktop.ini", "thumbs.db", ".ds_store"];
/// Thingiverse/MMF export boilerplate: kept, but not shown as documents.
pub const META_STEMS: &[&str] = &["readme", "read me", "license", "licence", "sources", "attribution card", "link"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Model,
    Scene,
    Image,
    Companion,
    Doc,
    Meta,
    Archive,
    Junk,
    Other,
}

/// The last path segment (`/` or `\` separated).
pub fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Split like Python's `os.path.splitext` on the basename: leading dots never start an extension.
fn split_ext(name: &str) -> (&str, &str) {
    let base = basename(name);
    let prefix = name.len() - base.len();
    match base.rfind('.') {
        Some(i) if base[..i].chars().any(|c| c != '.') => (&name[..prefix + i], &name[prefix + i..]),
        _ => (name, ""),
    }
}

/// Lowercase extension including the dot, or `""`.
pub fn ext(name: &str) -> String {
    split_ext(name).1.to_lowercase()
}

pub fn stem(name: &str) -> &str {
    split_ext(name).0
}

/// Classify a file name (or path; only the basename counts).
pub fn kind_of(name: &str) -> Kind {
    let base = basename(name);
    if JUNK_NAMES.contains(&base.to_lowercase().as_str()) {
        return Kind::Junk;
    }
    let e = ext(base);
    let e = e.as_str();
    if matches!(e, ".txt" | ".html" | ".htm" | ".md") && META_STEMS.contains(&normalize(stem(base)).as_str()) {
        return Kind::Meta;
    }
    for (exts, kind) in [
        (MODEL_EXTS, Kind::Model),
        (SCENE_EXTS, Kind::Scene),
        (IMAGE_EXTS, Kind::Image),
        (COMPANION_EXTS, Kind::Companion),
        (DOC_EXTS, Kind::Doc),
        (ARCHIVE_EXTS, Kind::Archive),
    ] {
        if exts.contains(&e) {
            return kind;
        }
    }
    Kind::Other
}

static NON_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\W_]+").unwrap());
static PART_VOLUME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.part0*(\d+)\.rar$").unwrap());

/// Lowercase, runs of punctuation/underscores become single spaces.
pub fn normalize(name: &str) -> String {
    NON_WORD.replace_all(&name.to_lowercase(), " ").trim().to_string()
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Model => "model",
            Kind::Scene => "scene",
            Kind::Image => "image",
            Kind::Companion => "companion",
            Kind::Doc => "doc",
            Kind::Meta => "meta",
            Kind::Archive => "archive",
            Kind::Junk => "junk",
            Kind::Other => "other",
        }
    }
}

pub fn is_printable(name: &str) -> bool {
    matches!(kind_of(name), Kind::Model | Kind::Scene)
}

/// Python's `str.isdigit()` for the names we meet: non-empty and all ASCII digits.
pub fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

pub fn squash(name: &str) -> String {
    normalize(name).replace(' ', "")
}

pub fn tokens(name: &str) -> Vec<String> {
    normalize(name).split_whitespace().map(str::to_string).collect()
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// `s` without its last char.
fn drop_last_char(s: &str) -> &str {
    s.char_indices().next_back().map_or(s, |(i, _)| &s[..i])
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

// ---------------------------------------------------------------- variants

static SCALE: LazyLock<Regex> = LazyLock::new(|| re(r"\b(\d{2,3}) ?mm\b"));
static BY: LazyLock<Regex> = LazyLock::new(|| re(r"\s+by\s+\w+(\s+\w+)?$"));
static LEAD_NUM: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{1,2} "));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
static TRAIL_NUM: LazyLock<Regex> = LazyLock::new(|| re(r" \d{1,2}$"));
static LEADING: LazyLock<Regex> = LazyLock::new(|| re(r"^(pre ?supported|supported|support|unsupported) (.+)$"));
/// Endings matched on the squashed name (no word boundary needed: "GordonSupported").
static SQUASHED_SUFFIXES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    const FILES_TAIL: &str = r"(stls?|objs?|files?|lys)?$";
    [
        (r"(un|non)supp?or?ted?", "unsupported"),
        (r"(pres?)?supp?or?ted?", "supported"),
        (r"lychee(scenes?|files?)?", "lychee"),
        (r"originalfiles", "original"),
        (r"raw(stls?|objs?|files?)", "raw"),
    ]
    .into_iter()
    .map(|(p, l)| (re(&format!("{p}{FILES_TAIL}")), l))
    .collect()
});
/// Whole trailing tokens (need a word boundary).
fn token_suffix(t: &str) -> Option<&'static str> {
    Some(match t {
        "stl" | "stls" => "stl",
        "obj" | "objs" => "obj",
        "lys" | "lychee" => "lychee",
        "chitubox" => "chitubox",
        "raw" => "raw",
        "original" => "original",
        "finished" => "finished",
        // abbreviations on archive names: "RoK_AGG_STL_SUP.zip"
        "sup" | "supp" | "presup" => "supported",
        "unsup" => "unsupported",
        // hollow vs solid casts ("Aurelion/Hueco" + "Aurelion/Normal")
        "hollow" | "hollowed" | "hueco" => "hollow",
        "solid" | "macizo" => "solid",
        _ => return None,
    })
}
const FORMAT_WORDS: &[&str] = &["stl", "stls", "obj", "objs", "format", "files", "raw", "update"];

/// `Some((label, prefix))` if the folder name denotes a print variant. `prefix` is the remaining
/// (normalized) model-name part, `""` if the name is only a variant.
pub fn variant_of(name: &str) -> Option<(String, String)> {
    let n = normalize(&FORMAT.replace_all(name, ""));
    let n = LEAD_NUM.replace(&n, "").into_owned(); // "01. FORMAT STL"
    let n = BY.replace(&n, "").into_owned(); // "Pressupported - By VeX"
    let scale = SCALE.captures(&n).map(|c| c[1].to_string());
    let n = if scale.is_some() { SCALE.replace_all(&n, " ").into_owned() } else { n };
    let n = SPACES.replace_all(&n, " ").trim().to_string();
    let n = n.strip_prefix("format ").unwrap_or(&n).to_string();
    let mut found = variant_core(&n);
    if found.is_none() && TRAIL_NUM.is_match(&n) {
        // split parts: "Unsupported_1"
        found = variant_core(&TRAIL_NUM.replace(&n, ""));
    }
    let (label, prefix) = found?;
    Some(match scale {
        Some(s) => (format!("{label} {s}mm"), prefix),
        None => (label, prefix),
    })
}

fn variant_core(n: &str) -> Option<(String, String)> {
    let mut label: Option<String> = None;
    let mut prefix = String::new();
    let sq = n.replace(' ', "");
    for (pat, lab) in SQUASHED_SUFFIXES.iter() {
        if let Some(m) = pat.find(&sq) {
            label = Some(lab.to_string());
            prefix = unsquash_prefix(n, m.start()).to_string();
            break;
        }
    }
    if label.is_none() {
        let toks: Vec<&str> = n.split_whitespace().collect();
        if let Some(l) = toks.last().and_then(|t| token_suffix(t)) {
            label = Some(l.to_string());
            prefix = toks[..toks.len() - 1].join(" ");
        }
    }
    if label.is_some() {
        // "raw stl", "update stls", "RoK_AGG_STL_SUP"
        let mut ptoks: Vec<&str> = prefix.split_whitespace().collect();
        while ptoks.last().is_some_and(|t| FORMAT_WORDS.contains(t)) {
            ptoks.pop();
        }
        prefix = ptoks.join(" ");
    }
    let subject = if label.is_some() { prefix.clone() } else { n.to_string() };
    if let Some(m) = LEADING.captures(&subject) {
        // "Supported Backpacks STL" -> merges with sibling "Backpacks"
        let lead = if m[1].starts_with("un") { "unsupported" } else { "supported" };
        label = Some(match label {
            None => lead.to_string(),
            Some(l) => format!("{l} {lead}"),
        });
        prefix = m[2].to_string();
    }
    label.map(|l| (l, prefix.trim().to_string()))
}

/// Map a byte offset in the squashed (spaces removed) string back to the spaced string's prefix.
fn unsquash_prefix(n: &str, squashed_index: usize) -> &str {
    let mut count = 0;
    for (i, ch) in n.char_indices() {
        if ch == ' ' {
            continue;
        }
        if count == squashed_index {
            return &n[..i];
        }
        count += ch.len_utf8();
    }
    n
}

const SUPPORT_ORDER: &[&str] = &["supported", "unsupported", "lychee", "chitubox", "finished"];

/// `"stl 25mm"` → `{25mm}`; `"raw"` → `{unsupported}`; format words are dropped (the file
/// extension already says it).
pub fn variant_atoms(label: &str) -> BTreeSet<String> {
    label
        .split_whitespace()
        .filter_map(|t| match t {
            "raw" | "original" => Some("unsupported"),
            "stl" | "obj" => None,
            t => Some(t),
        })
        .map(str::to_string)
        .collect()
}

pub fn variant_label(atoms: &BTreeSet<String>) -> String {
    let support = SUPPORT_ORDER.iter().filter(|a| atoms.contains(**a)).map(|a| a.to_string());
    let rest = atoms.iter().filter(|a| !SUPPORT_ORDER.contains(&a.as_str())).cloned();
    support.chain(rest).collect::<Vec<_>>().join("+")
}

static GDRIVE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| re(r"-\d{8}T\d{6}Z-\d{3}$"));

/// Remove download artefacts from folder names (Google Drive zip export suffix).
pub fn clean_segment(name: &str) -> String {
    let s = GDRIVE_SUFFIX.replace(name, "");
    let s = s.trim();
    if s.is_empty() { name.to_string() } else { s.to_string() }
}

/// True if every token of `prefix` appears in `other` (or it is a squashed substring).
pub fn contained_in(prefix: &str, other: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let ot = tokens(other);
    if prefix.split_whitespace().all(|t| ot.iter().any(|o| o == t)) {
        return true;
    }
    squash(other).contains(&prefix.replace(' ', ""))
}

// ---------------------------------------------------------------- part groups

const PART_WORDS: &[&str] = &[
    "arm", "arms", "head", "heads", "helmet", "helmets", "leg", "legs", "torso", "torsos", "body", "bodies",
    "backpack", "backpacks", "base", "bases", "weapon", "weapons", "gun", "guns", "pad", "pads", "shoulderpad",
    "shoulderpads", "shoulder", "shoulders", "bit", "bits", "blade", "blades", "boltgun", "boltguns", "hand", "hands",
    "cannon", "cannons", "armor", "armour", "wing", "wings", "tail", "tails", "cloak", "cloaks", "cape", "capes",
    "accessory", "accessories", "part", "parts", "pose", "poses", "shield", "shields", "sword", "swords", "ammunition",
    "ammo", "fx", "gubbins", "drone", "drones", "jetpack", "chest", "hull", "turret", "turrets", "tracks", "sprue",
    "misc", "extras", "options", "addons", "upgrades", "upg", "bitz", "pipe", "pipes", "files", "pieces", "fist",
    "fists",
    // Portuguese / Spanish (Skydrake kit)
    "cabeca", "bracos", "braco", "asas", "corpo", "pernas", "brazos", "cabeza", "cuerpo", "piernas",
];
const PART_MODIFIERS: &[&str] = &[
    "l", "r", "left", "right", "empty", "old", "new", "extra", "alt", "alternative", "and", "with", "main", "front",
    "back", "rear", "upper", "lower", "big", "small", "one", "two", "power", "fist", "melee", "ranged", "add", "on",
    "ons",
];

pub fn is_part_group(name: &str) -> bool {
    let toks: Vec<String> = tokens(name).into_iter().filter(|t| !is_digits(t)).collect();
    if toks.is_empty() {
        return false;
    }
    let part = |t: &String| PART_WORDS.contains(&t.as_str());
    if toks.iter().all(|t| part(t) || PART_MODIFIERS.contains(&t.as_str())) && toks.iter().any(part) {
        return true;
    }
    toks.len() <= 2 && toks.iter().any(part) // "arms-Empty", "Legs DiscKnees"
}

// ---------------------------------------------------------------- images

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRole {
    Preview,
    Thumb,
    Instructions,
}

impl ImageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageRole::Preview => "preview",
            ImageRole::Thumb => "thumb",
            ImageRole::Instructions => "instructions",
        }
    }
}

const INSTRUCTION_WORDS: &[&str] = &[
    "instructions", "instruction", "inst", "assembly", "guide", "buildguide", "contents", "partlist", "painting",
    "howto", "manual", "sprues",
];
const INSTRUCTION_PHRASES: &[&str] = &["part list", "parts list", "build guide", "how to"];

/// Preview, instructions, or a per-file thumbnail like `Pose A.stl.png`.
pub fn image_role(name: &str) -> ImageRole {
    if is_printable(stem(name)) {
        return ImageRole::Thumb;
    }
    let n = normalize(stem(name));
    if n.split_whitespace().any(|t| INSTRUCTION_WORDS.contains(&t)) || INSTRUCTION_PHRASES.iter().any(|p| n.contains(p)) {
        return ImageRole::Instructions;
    }
    ImageRole::Preview
}

static GENERIC: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^(?:(photo|image|img|untitled|render|preview|screenshot|pic|picture|cover|poster|thumb|thumbnail|main|banner)?\s*[\d ]*)$")
});
static HEX_HASH: LazyLock<Regex> = LazyLock::new(|| re(r"^[0-9a-f]{16,}"));
static ANY_DIGIT: LazyLock<Regex> = LazyLock::new(|| re(r"\d"));
static ASCII_LOWER: LazyLock<Regex> = LazyLock::new(|| re(r"[a-z]"));
static ASCII_UPPER: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Z]"));

pub fn is_generic_image(name: &str) -> bool {
    let s = stem(name);
    if GENERIC.is_match(&normalize(s)) {
        return true;
    }
    if HEX_HASH.is_match(&s.to_lowercase()) {
        // thingiverse hashes
        return true;
    }
    // vk/telegram-style ids: "LsB0hMMwR5E", "-qAyp6h5X38"
    let core = s.trim_matches(['-', '_']);
    !s.contains(' ')
        && (8..=16).contains(&char_len(core))
        && ANY_DIGIT.is_match(core)
        && ASCII_LOWER.is_match(core)
        && ASCII_UPPER.is_match(core)
}

static LETTER_DIGIT: LazyLock<Regex> = LazyLock::new(|| re(r"^.*[a-z]\d$"));

/// Normalized name for pairing a picture with an archive/folder: `Eternal Falcon (3).jpg`,
/// `Cyber_Raider1.jpg`, `Arcanus_Knight_Paladin-1_2.webp` → core name.
pub fn sidecar_key(name: &str) -> String {
    let base = if matches!(kind_of(name), Kind::Image | Kind::Archive) { stem(name) } else { name };
    let mut toks: Vec<String> = normalize(base).split_whitespace().map(str::to_string).collect();
    while toks.len() > 1 && toks.last().is_some_and(|t| is_digits(t) && char_len(t) <= 2) {
        toks.pop();
    }
    if let Some(last) = toks.last_mut()
        && LETTER_DIGIT.is_match(last)
        && char_len(last) > 3
    {
        // "viking1", "weapons2"
        *last = drop_last_char(last).to_string();
    }
    toks.iter().map(|t| singular(t)).collect::<Vec<_>>().join(" ")
}

pub fn singular(t: &str) -> String {
    if char_len(t) > 3 && t.ends_with('s') && !t.ends_with("ss") { drop_last_char(t).to_string() } else { t.to_string() }
}

const IMAGE_NOISE: &[&str] = &["complete", "resize", "mmf", "v2", "v3", "hero", "render", "renders", "preview", "featured"];

/// Tokens of an image stem with the noise removed (`resize-`, `-01`, ` (3)`, `complete`).
pub fn image_tokens(name: &str) -> Vec<String> {
    let n = normalize(stem(name));
    let n = n.strip_prefix("resize ").unwrap_or(&n);
    let mut toks: Vec<String> =
        n.split_whitespace().filter(|t| !IMAGE_NOISE.contains(t)).map(str::to_string).collect();
    while toks.last().is_some_and(|t| is_digits(t) && char_len(t) <= 2) {
        toks.pop();
    }
    toks
}

// ---------------------------------------------------------------- model names

/// `_CATALOG` without its lookahead: the separator it required is consumed by the trailing class.
static CATALOG: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*(\d{3,4})(?:\s*-\s*\d{3,4})?[\s_\-.]+"));
static HL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(HL\d+)$"));
static EDITION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\s*\((\d{1,2}E)\)"));
static FORMAT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\s*\(format \w+\)"));
static BY_DESIGNER: LazyLock<Regex> = LazyLock::new(|| re(r"\s*-?\s*\b[Bb]y\s+([A-Z]\w*(?:\s+[A-Z]\w*)?)\s*$"));
static RELEASE_TAIL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i),\s*[^,]*\b(release|update)\b[^,]*$"));
static LEAD_PAREN_NUM: LazyLock<Regex> = LazyLock::new(|| re(r"^\(\d{1,2}\)\s*"));
static COMPLETE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^complete\s+"));
static UNDERSCORE_PLUS: LazyLock<Regex> = LazyLock::new(|| re(r"[_+]+"));
static PAREN_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"\(([^()\s]{3,})\)"));

/// Name attributes pulled out by [`parse_name`] (and catalog ids adopted from pictures).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attrs {
    pub catalog_id: Option<String>,
    pub edition: Option<String>,
    pub designer: Option<String>,
}

impl Attrs {
    /// JSON object holding only the attributes that are set.
    pub fn to_json(&self) -> String {
        let fields: Vec<String> = [("catalog_id", &self.catalog_id), ("edition", &self.edition), ("designer", &self.designer)]
            .into_iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| format!("{}: {}", json_str(k), json_str(v))))
            .collect();
        format!("{{{}}}", fields.join(", "))
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn catalog_id(name: &str) -> Option<String> {
    if let Some(c) = HL.captures(name.trim()) {
        return Some(c[1].to_uppercase());
    }
    CATALOG.captures(name).map(|c| c[1].to_string())
}

/// Split a raw folder name into its display name and attributes.
pub fn parse_name(name: &str, source_name: Option<&str>, designer_tags: &[String]) -> (String, Attrs) {
    let mut attrs = Attrs::default();
    let mut s = LEAD_PAREN_NUM.replace(name, "").into_owned(); // "(1) Tal'Ri"
    if let Some(cid) = catalog_id(&s) {
        attrs.catalog_id = Some(cid);
        if !HL.is_match(s.trim()) {
            s = CATALOG.replace(&s, "").into_owned();
            s = COMPLETE.replace(&s, "").into_owned(); // Grimlax "842 Complete Spiderwalker"
        }
    }
    if let Some(c) = EDITION.captures(&s) {
        attrs.edition = Some(c[1].to_uppercase());
        s = EDITION.replace_all(&s, "").into_owned();
    }
    s = FORMAT.replace_all(&s, "").into_owned();
    if let Some(c) = BY_DESIGNER.captures(&s) {
        attrs.designer = Some(c[1].to_string());
        let start = c.get(0).expect("whole match").start();
        s.truncate(start);
    }
    for tag in designer_tags {
        let t = format!("({tag})");
        if s.contains(&t) {
            attrs.designer = Some(tag.clone());
            s = s.replace(&t, "");
        }
    }
    s = RELEASE_TAIL.replace(&s, "").into_owned();
    if let Some(src) = source_name
        && normalize(&s) != normalize(src)
    {
        let pat = re(&format!(r"(?i){}\s*-\s*", regex::escape(src)));
        s = pat.replace_all(&s, "").into_owned();
    }
    s = UNDERSCORE_PLUS.replace_all(&s, " ").into_owned(); // "Ancient+Alien+Pylon"
    let s = SPACES.replace_all(&s, " ");
    let s = s.trim_matches([' ', '-', '.', ',']);
    let display = if s.is_empty() { name.trim().to_string() } else { s.to_string() };
    (display, attrs)
}

pub fn parenthetical_tags(name: &str) -> Vec<String> {
    PAREN_TAG.captures_iter(name).map(|c| c[1].to_string()).collect()
}

/// The volume name of an added folder: its last path component, or `D:` for a drive root.
pub fn volume_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let last = basename(trimmed);
    if last.is_empty() { path.to_string() } else { last.to_string() }
}

/// Archive display name: the stem without a `.part1`/`.part01`/`.part001` suffix.
pub fn archive_stem(file_name: &str) -> String {
    let s = stem(file_name);
    for suffix in [".part1", ".part01", ".part001"] {
        if s.to_lowercase().ends_with(suffix) {
            return s[..s.len() - suffix.len()].to_string();
        }
    }
    s.to_string()
}

/// `x.part2.rar`, `x.part03.rar`…: continuation volumes of a multi-part RAR, not sources of their own.
pub fn is_secondary_rar_volume(file_name: &str) -> bool {
    PART_VOLUME.captures(file_name).is_some_and(|c| c[1].parse::<u32>().is_ok_and(|n| n != 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        let cases = [
            ("a.STL", Kind::Model),
            ("x.lys", Kind::Scene),
            ("p.JPEG", Kind::Image),
            ("m.mtl", Kind::Companion),
            ("guide.pdf", Kind::Doc),
            ("README.txt", Kind::Meta),
            ("attribution_card.html", Kind::Meta),
            ("notes.txt", Kind::Doc),
            ("k.7z", Kind::Archive),
            ("Desktop.ini", Kind::Junk),
            ("f.obv", Kind::Other),
            ("Pack/inner.7z", Kind::Archive),
        ];
        for (name, kind) in cases {
            assert_eq!(kind_of(name), kind, "{name}");
        }
    }

    #[test]
    fn ext_and_stem() {
        assert_eq!(ext("A.B.STL"), ".stl");
        assert_eq!(stem("A.B.STL"), "A.B");
        assert_eq!(ext(".hidden"), "");
        assert_eq!(stem("noext"), "noext");
    }

    #[test]
    fn normalize_punctuation() {
        assert_eq!(normalize("Foo_Bar-Baz  (1)"), "foo bar baz 1");
        assert_eq!(normalize("ГОБЛИНЫ_воин"), "гоблины воин");
    }

    #[test]
    fn volume_name_of_drive_root() {
        assert_eq!(volume_name("D:\\"), "D:");
        assert_eq!(volume_name("D:\\Models\\"), "Models");
        assert_eq!(volume_name("D:\\Models\\Vol.1"), "Vol.1");
    }

    #[test]
    fn rar_volumes() {
        assert_eq!(archive_stem("Kit.part01.rar"), "Kit");
        assert_eq!(archive_stem("Kit.zip"), "Kit");
        assert!(is_secondary_rar_volume("Kit.part2.rar"));
        assert!(!is_secondary_rar_volume("Kit.part1.rar"));
        assert!(!is_secondary_rar_volume("Kit.part001.rar"));
        assert!(!is_secondary_rar_volume("Kit.rar"));
    }
}
