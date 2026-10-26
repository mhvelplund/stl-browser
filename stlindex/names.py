"""Name heuristics: file kinds, variant / part-group / image vocabularies, name cleanup."""
import os
import re

MODEL_EXTS = {'.stl', '.obj', '.3mf', '.ply', '.fbx', '.blend', '.ztl', '.zpr', '.step', '.stp'}
SCENE_EXTS = {'.lys', '.lyt', '.chitubox', '.ctb', '.cbddlp', '.goo', '.photon', '.pwmo', '.pwms', '.sl1'}
IMAGE_EXTS = {'.jpg', '.jpeg', '.png', '.webp', '.gif', '.bmp'}
COMPANION_EXTS = {'.mtl'}
DOC_EXTS = {'.pdf', '.txt', '.md', '.html', '.htm', '.rtf'}
ARCHIVE_EXTS = {'.zip', '.rar', '.7z'}
JUNK_NAMES = {'desktop.ini', 'thumbs.db', '.ds_store'}
# Thingiverse/MMF export boilerplate: kept, but not shown as documents
META_STEMS = {'readme', 'read me', 'license', 'licence', 'sources', 'attribution card', 'link'}


def ext(name):
    return os.path.splitext(name)[1].lower()


def stem(name):
    return os.path.splitext(name)[0]


def kind_of(name):
    """Classify a file name: model | scene | image | companion | doc | meta | archive | junk | other."""
    if name.lower() in JUNK_NAMES:
        return 'junk'
    e = ext(name)
    if e in ('.txt', '.html', '.htm', '.md') and normalize(stem(name)) in META_STEMS:
        return 'meta'
    if e in MODEL_EXTS:
        return 'model'
    if e in SCENE_EXTS:
        return 'scene'
    if e in IMAGE_EXTS:
        return 'image'
    if e in COMPANION_EXTS:
        return 'companion'
    if e in DOC_EXTS:
        return 'doc'
    if e in ARCHIVE_EXTS:
        return 'archive'
    return 'other'


def is_printable(name):
    return kind_of(name) in ('model', 'scene')


def normalize(name):
    """Lowercase, punctuation/underscores -> single spaces."""
    return re.sub(r'[\W_]+', ' ', name.lower()).strip()


def squash(name):
    return normalize(name).replace(' ', '')


def tokens(name):
    return normalize(name).split()


# ---------------------------------------------------------------- variants

_SCALE = re.compile(r'\b(\d{2,3}) ?mm\b')
_BY = re.compile(r'\s+by\s+\w+(\s+\w+)?$')
# endings matched on the squashed name (no word boundary needed: "GordonSupported")
_SQUASHED_SUFFIXES = [
    (r'(un|non)supp?or?ted?', 'unsupported'),
    (r'(pres?)?supp?or?ted?', 'supported'),
    (r'lychee(scenes?|files?)?', 'lychee'),
    (r'originalfiles', 'original'),
    (r'raw(stls?|objs?|files?)', 'raw'),
]
_FILES_TAIL = r'(stls?|objs?|files?|lys)?'
# whole trailing tokens (need a word boundary)
_TOKEN_SUFFIXES = {
    'stl': 'stl', 'stls': 'stl', 'obj': 'obj', 'objs': 'obj', 'lys': 'lychee', 'lychee': 'lychee',
    'chitubox': 'chitubox', 'raw': 'raw', 'original': 'original', 'finished': 'finished',
    # abbreviations on archive names: "RoP_AGG_STL_SUP.zip"
    'sup': 'supported', 'supp': 'supported', 'presup': 'supported', 'unsup': 'unsupported',
    # hollow vs solid casts ("Sanguinus/Hueco" + "Sanguinus/Normal")
    'hollow': 'hollow', 'hollowed': 'hollow', 'hueco': 'hollow', 'solid': 'solid', 'macizo': 'solid',
}
_FORMAT_WORDS = {'stl', 'stls', 'obj', 'objs', 'format', 'files', 'raw', 'update'}
_LEADING = re.compile(r'^(pre ?supported|supported|support|unsupported) (.+)$')


def variant_of(name):
    """Return (label, prefix) if the folder name denotes a print variant, else None.

    prefix is the remaining (normalized) model-name part, '' if the name is only a variant.
    """
    n = normalize(_FORMAT.sub('', name))
    n = re.sub(r'^\d{1,2} ', '', n)          # "01. FORMAT STL"
    n = _BY.sub('', n)                        # "Pressupported - By AiR"
    scale = _SCALE.search(n)
    if scale:
        n = _SCALE.sub(' ', n)
    n = re.sub(r'^format ', '', re.sub(r'\s+', ' ', n).strip())
    found = _variant_core(n)
    if found is None and re.search(r' \d{1,2}$', n):    # split parts: "Unsupported_1"
        found = _variant_core(re.sub(r' \d{1,2}$', '', n))
    if found is None:
        return None
    label, prefix = found
    if scale:
        label = f'{label} {scale.group(1)}mm'
    return label, prefix


def _variant_core(n):
    label = prefix = None
    sq = n.replace(' ', '')
    for pat, lab in _SQUASHED_SUFFIXES:
        m = re.search(pat + _FILES_TAIL + '$', sq)
        if m:
            label, prefix = lab, _unsquash_prefix(n, m.start())
            break
    if label is None:
        toks = n.split()
        if toks and toks[-1] in _TOKEN_SUFFIXES:
            label, prefix = _TOKEN_SUFFIXES[toks[-1]], ' '.join(toks[:-1])
    if label is not None:              # "raw stl", "update stls", "RoP_AGG_STL_SUP"
        ptoks = prefix.split()
        while ptoks and ptoks[-1] in _FORMAT_WORDS:
            ptoks.pop()
        prefix = ' '.join(ptoks)
    m = _LEADING.match(prefix if label is not None else n)
    if m:                              # "Supported Backpacks STL" -> merges with sibling "Backpacks"
        lead = 'unsupported' if m.group(1).startswith('un') else 'supported'
        label = lead if label is None else f'{label} {lead}'
        prefix = m.group(2)
    if label is None:
        return None
    return label, prefix.strip()


_SUPPORT_ORDER = ['supported', 'unsupported', 'lychee', 'chitubox', 'finished']
_ATOM_ALIASES = {'raw': 'unsupported', 'original': 'unsupported', 'stl': None, 'obj': None}


def variant_atoms(label):
    """'stl 25mm' -> {'25mm'}; 'raw' -> {'unsupported'}; format words are dropped (the file
    extension already says it)."""
    out = set()
    for t in label.split():
        t = _ATOM_ALIASES.get(t, t)
        if t:
            out.add(t)
    return out


def variant_label(atoms):
    support = [a for a in _SUPPORT_ORDER if a in atoms]
    rest = sorted(a for a in atoms if a not in _SUPPORT_ORDER)
    return '+'.join(support + rest)


_GDRIVE_SUFFIX = re.compile(r'-\d{8}T\d{6}Z-\d{3}$')


def clean_segment(name):
    """Remove download artefacts from folder names (Google Drive zip export suffix)."""
    return _GDRIVE_SUFFIX.sub('', name).strip() or name


def _unsquash_prefix(n, squashed_index):
    """Map an index in the squashed string back to the spaced string's prefix."""
    count = 0
    for i, ch in enumerate(n):
        if ch == ' ':
            continue
        if count == squashed_index:
            return n[:i]
        count += 1
    return n


def contained_in(prefix, other):
    """True if every token of prefix appears in other (or squashed substring)."""
    if not prefix:
        return True
    pt, ot = prefix.split(), set(tokens(other))
    if all(t in ot for t in pt):
        return True
    return prefix.replace(' ', '') in squash(other)


# ---------------------------------------------------------------- part groups

PART_WORDS = {
    'arm', 'arms', 'head', 'heads', 'helmet', 'helmets', 'leg', 'legs', 'torso', 'torsos', 'body',
    'bodies', 'backpack', 'backpacks', 'base', 'bases', 'weapon', 'weapons', 'gun', 'guns', 'pad',
    'pads', 'shoulderpad', 'shoulderpads', 'shoulder', 'shoulders', 'bit', 'bits', 'blade', 'blades',
    'boltgun', 'boltguns', 'hand', 'hands', 'cannon', 'cannons', 'armor', 'armour', 'wing', 'wings',
    'tail', 'tails', 'cloak', 'cloaks', 'cape', 'capes', 'accessory', 'accessories', 'part', 'parts',
    'pose', 'poses', 'shield', 'shields', 'sword', 'swords', 'ammunition', 'ammo', 'fx', 'gubbins',
    'drone', 'drones', 'jetpack', 'chest', 'hull', 'turret', 'turrets', 'tracks', 'sprue', 'misc',
    'extras', 'options', 'addons', 'upgrades', 'upg', 'bitz', 'pipe', 'pipes', 'files', 'pieces',
    'fist', 'fists',
    # Portuguese / Spanish (Heldrake kit)
    'cabeca', 'bracos', 'braco', 'asas', 'corpo', 'pernas', 'brazos', 'cabeza', 'cuerpo', 'piernas',
}
PART_MODIFIERS = {'l', 'r', 'left', 'right', 'empty', 'old', 'new', 'extra', 'alt', 'alternative',
                  'and', 'with', 'main', 'front', 'back', 'rear', 'upper', 'lower', 'big', 'small',
                  'one', 'two', 'power', 'fist', 'melee', 'ranged', 'add', 'on', 'ons'}


def is_part_group(name):
    toks = [t for t in tokens(name) if not t.isdigit()]
    if not toks:
        return False
    if all(t in PART_WORDS or t in PART_MODIFIERS for t in toks) and any(t in PART_WORDS for t in toks):
        return True
    return len(toks) <= 2 and any(t in PART_WORDS for t in toks)    # "arms-Empty", "Legs DiscKnees"


# ---------------------------------------------------------------- images

_INSTRUCTION_WORDS = {'instructions', 'instruction', 'inst', 'assembly', 'guide', 'buildguide',
                      'contents', 'partlist', 'painting', 'howto', 'manual', 'sprues'}
_INSTRUCTION_PHRASES = ('part list', 'parts list', 'build guide', 'how to')


def image_role(name):
    """preview | instructions | thumb (per-file thumbnail like 'Pose A.stl.png')."""
    if is_printable(stem(name)):
        return 'thumb'
    n = normalize(stem(name))
    if any(t in _INSTRUCTION_WORDS for t in n.split()) or any(p in n for p in _INSTRUCTION_PHRASES):
        return 'instructions'
    return 'preview'


_GENERIC = re.compile(r'(photo|image|img|untitled|render|preview|screenshot|pic|picture|cover|'
                      r'poster|thumb|thumbnail|main|banner)?\s*[\d ]*')


def is_generic_image(name):
    s = stem(name)
    n = normalize(s)
    if _GENERIC.fullmatch(n):
        return True
    if re.fullmatch(r'[0-9a-f]{16,}.*', s.lower()):                      # thingiverse hashes
        return True
    # vk/telegram-style ids: "LsB0hMMwR5E", "-qAyp6h5X38"
    core = s.strip('-_')
    return (' ' not in s and 8 <= len(core) <= 16 and re.search(r'\d', core) is not None
            and re.search(r'[a-z]', core) is not None and re.search(r'[A-Z]', core) is not None)


def sidecar_key(name):
    """Normalized name for pairing a picture with an archive/folder: 'Eternal Eagle (3).jpg',
    'Cyber_Viking1.jpg', 'Acasatus_Knight_Porphyrion-1_2.webp' -> core name."""
    toks = normalize(stem(name) if kind_of(name) in ('image', 'archive') else name).split()
    while len(toks) > 1 and toks[-1].isdigit() and len(toks[-1]) <= 2:
        toks.pop()
    if toks and re.fullmatch(r'.*[a-z]\d', toks[-1]) and len(toks[-1]) > 3:     # "viking1", "weapons2"
        toks[-1] = toks[-1][:-1]
    return ' '.join(singular(t) for t in toks)


def singular(t):
    return t[:-1] if len(t) > 3 and t.endswith('s') and not t.endswith('ss') else t


def image_tokens(name):
    """Tokens of an image stem with the noise removed ('resize-', '-01', ' (3)', 'complete')."""
    n = normalize(stem(name))
    n = re.sub(r'^resize ', '', n)
    toks = [t for t in n.split() if t not in ('complete', 'resize', 'mmf', 'v2', 'v3', 'hero', 'render',
                                               'renders', 'preview', 'featured')]
    while toks and toks[-1].isdigit() and len(toks[-1]) <= 2:
        toks.pop()
    return toks


# ---------------------------------------------------------------- model names

_CATALOG = re.compile(r'^\s*(\d{3,4})(?:\s*-\s*\d{3,4})?(?=[\s_\-.])[\s_\-.]*')
_HL = re.compile(r'^(HL\d+)$', re.I)
_EDITION = re.compile(r'\s*\((\d{1,2}E)\)', re.I)
_FORMAT = re.compile(r'\s*\(format \w+\)', re.I)
_BY_DESIGNER = re.compile(r'\s*-?\s*\b[Bb]y\s+([A-Z]\w*(?:\s+[A-Z]\w*)?)\s*$')
_RELEASE_TAIL = re.compile(r',\s*[^,]*\b(release|update)\b[^,]*$', re.I)


def catalog_id(name):
    m = _HL.match(name.strip())
    if m:
        return m.group(1).upper()
    m = _CATALOG.match(name)
    return m.group(1) if m else None


def parse_name(name, source_name=None, designer_tags=()):
    """Split a raw folder name into (display_name, attrs dict)."""
    attrs = {}
    s = re.sub(r'^\(\d{1,2}\)\s*', '', name)       # "(1) Shas'Ui"
    cid = catalog_id(s)
    if cid:
        attrs['catalog_id'] = cid
        if not _HL.match(s.strip()):
            s = _CATALOG.sub('', s, count=1)
            s = re.sub(r'^complete\s+', '', s, flags=re.I)     # Ghamak "842 Complete Tarantula"
    m = _EDITION.search(s)
    if m:
        attrs['edition'] = m.group(1).upper()
        s = _EDITION.sub('', s)
    s = _FORMAT.sub('', s)
    m = _BY_DESIGNER.search(s)
    if m:
        attrs['designer'] = m.group(1)
        s = s[:m.start()]
    for tag in designer_tags:
        if f'({tag})' in s:
            attrs['designer'] = tag
            s = s.replace(f'({tag})', '')
    s = _RELEASE_TAIL.sub('', s)
    if source_name and normalize(s) != normalize(source_name):
        s = re.sub(re.escape(source_name) + r'\s*-\s*', '', s, flags=re.I)
    s = re.sub(r'[_+]+', ' ', s)                   # "Ancient+Alien+Pylon"
    s = re.sub(r'\s+', ' ', s).strip(' -.,')
    return (s or name.strip()), attrs


def parenthetical_tags(name):
    return re.findall(r'\(([^()\s]{3,})\)', name)
