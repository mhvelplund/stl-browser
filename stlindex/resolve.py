"""Turn the flat file list of one source (archive or folder) into groups / models / files / assets.

Pipeline (see archive-patterns-plan.md):
  physical tree -> variant marking (incl. sibling rule) -> logical tree (variant folders made
  transparent, image-only folders hoisted) -> model detection (wrappers collapsed, part-kits
  recognised) -> asset attachment (thumb / id / name / only-child / poster).
"""
from collections import Counter
from dataclasses import dataclass, field

from . import names as N

STRUCTURAL_KINDS = ('model', 'scene', 'archive')     # an unexpanded archive stands in for models
ROLE_ORDER = {'preview': 0, 'thumb': 1, 'instructions': 2, 'doc': 3}
ROLE_EXCLUDE_DESIGNER = {'male', 'female', 'leader', 'banner', 'musician', 'champion', 'sergeant',
                         'command', 'unit', 'squad', 'supported', 'unsupported', 'presupported'}


@dataclass
class Entry:
    id: int
    path: str          # '/'-separated, nested archives appear as a path segment ("a/x.zip/b.stl")
    size: int = 0


# ---------------------------------------------------------------- physical tree

@dataclass(eq=False)
class PDir:
    name: str
    children: dict = field(default_factory=dict)
    files: list = field(default_factory=list)     # (Entry, filename)
    structural: bool = False
    variant: tuple = None                          # (label, prefix)


def _build_physical(entries):
    root = PDir('')
    expanded = set()
    for e in entries:
        parts = [p for p in e.path.split('/') if p]
        node = root
        for seg in parts[:-1]:
            if N.kind_of(seg) == 'archive':          # nested archive expanded in place: "x.zip/" -> "x/"
                expanded.add((id(node), seg.lower()))
                seg = N.stem(seg)
            seg = N.clean_segment(seg)
            node = node.children.setdefault(seg, PDir(seg))
        node.files.append((e, parts[-1]))
    _drop_expanded(root, expanded)
    _mark_structural(root)
    return root


def _drop_expanded(node, expanded):
    """Remove the archive-file entries whose contents are already present as a folder."""
    node.files = [(e, f) for e, f in node.files if (id(node), f.lower()) not in expanded]
    for child in node.children.values():
        _drop_expanded(child, expanded)


def _mark_structural(node):
    s = any(N.kind_of(f) in STRUCTURAL_KINDS for _, f in node.files)
    for c in node.children.values():
        s = _mark_structural(c) or s
    node.structural = s
    return s


def _mark_variants(node):
    kids = list(node.children.values())
    for c in kids:
        c.variant = N.variant_of(c.name) if c.structural else None
    prefixes = {c.variant[1] for c in kids if c.variant and c.variant[1]}
    hollow = any(c.variant and 'hollow' in c.variant[0] for c in kids)
    for c in kids:
        # sibling rule: "Command Force" next to "Command Force Supported" is the unsupported variant
        if c.structural and not c.variant and N.normalize(c.name) in prefixes:
            c.variant = ('unsupported', N.normalize(c.name))
        elif c.structural and not c.variant and hollow and N.normalize(c.name) in ('normal', 'full'):
            c.variant = ('solid', '')          # "Normal" next to "Hueco" (hollow)
    for c in kids:
        _mark_variants(c)


# ---------------------------------------------------------------- logical tree

@dataclass(eq=False)
class LFile:
    entry: Entry
    name: str
    kind: str
    variant: frozenset = frozenset()


@dataclass(eq=False)
class LNode:
    name: str
    parent: 'LNode' = None
    children: dict = field(default_factory=dict)
    files: list = field(default_factory=list)
    structural: bool = False
    model: object = None        # Model owning this node (set on model node and its subtree)
    group: object = None        # Group created for this node
    models_below: list = field(default_factory=list)

    def child(self, key, display):
        c = self.children.get(key)
        if c is None:
            c = self.children[key] = LNode(display, self)
        return c

    def walk(self):
        yield self
        for c in self.children.values():
            yield from c.walk()

    def path(self):
        out, n = [], self
        while n.parent is not None:
            out.append(n.name)
            n = n.parent
        return '/'.join(reversed(out))


def _display_prefix(name, prefix):
    """The longest leading part of the original name whose normalized form equals prefix."""
    best = None
    for i in range(1, len(name) + 1):
        if N.normalize(name[:i]) == prefix:
            best = name[:i]
    return (best or prefix).strip(' _-.')


def _add(labels, label):
    return labels | N.variant_atoms(label)


def _hoist(pdir, lnode, labels):
    for e, f in pdir.files:
        lnode.files.append(LFile(e, f, N.kind_of(f), labels))
    for c in pdir.children.values():
        _hoist(c, lnode, labels)


def _build_logical(pdir, lnode, labels, ancestors):
    for e, f in pdir.files:
        lnode.files.append(LFile(e, f, N.kind_of(f), labels))
    only_variant_siblings = sum(1 for c in pdir.children.values() if c.structural and not c.variant) == 1
    for c in pdir.children.values():
        if not c.structural:                         # Renders/, Poster/, images/ ...
            _hoist(c, lnode, labels)
            continue
        if c.variant:
            label, prefix = c.variant
            if any(N.contained_in(prefix, a) for a in ancestors[-4:]):
                _build_logical(c, lnode, _add(labels, label), ancestors)
            else:                                    # "569 X presupported": model folder + variant
                target = lnode.child(prefix, _display_prefix(c.name, prefix))
                _build_logical(c, target, _add(labels, label), ancestors + [c.name])
        elif (only_variant_siblings and N.normalize(c.name)
              and any(N.contained_in(N.normalize(c.name), a) for a in ancestors[-2:])):
            # "578-590_Hoplites of Light/Hoplites of Light/" next to "presupported/": a repeat of
            # the parent name is just the default variant's container
            _build_logical(c, lnode, labels, ancestors)
        else:
            target = lnode.child(N.normalize(c.name) or c.name, c.name)
            _build_logical(c, target, labels, ancestors + [c.name])


def _mark_logical_structural(node):
    s = any(f.kind in STRUCTURAL_KINDS for f in node.files)
    for c in node.children.values():
        s = _mark_logical_structural(c) or s
    node.structural = s
    return s


# ---------------------------------------------------------------- results

@dataclass(eq=False)
class Group:
    id: int
    name: str
    raw_name: str
    path: str
    parent: 'Group' = None
    attrs: dict = field(default_factory=dict)
    node: object = None


@dataclass(eq=False)
class Model:
    id: int
    name: str
    raw_name: str
    path: str
    group: Group = None
    attrs: dict = field(default_factory=dict)
    files: list = field(default_factory=list)       # ModelFile
    assets: list = field(default_factory=list)      # Asset
    node: LNode = None

    @property
    def variants(self):
        return sorted({f.variant for f in self.files if f.kind in ('model', 'scene')})

    @property
    def hero(self):
        return self.assets[0] if self.assets and self.assets[0].kind == 'image' else None


@dataclass(eq=False)
class ModelFile:
    entry: Entry
    name: str
    kind: str
    part_group: str
    variant: str


@dataclass(eq=False)
class Asset:
    entry: Entry
    name: str
    kind: str                # image | doc
    role: str                # preview | thumb | instructions | poster | doc
    model: Model = None
    group: Group = None
    generic: bool = False
    depth: int = 0
    match: str = ''          # how it was attached: inside | thumb | id | name | only | poster


@dataclass
class Result:
    source_name: str
    groups: list
    models: list
    assets: list             # all assets incl. group/source posters


# ---------------------------------------------------------------- model detection

def _has_direct(node):
    return any(f.kind in STRUCTURAL_KINDS for f in node.files)


def _structural_kids(node):
    return [c for c in node.children.values() if c.structural]


class _Resolver:
    def __init__(self, source_name, force_models=()):
        self.source_name = source_name
        self.force_models = set(force_models)      # group nodes to treat as one model (units)
        self.groups, self.models = [], []

    def name_of(self, node):
        return node.name if node.parent is not None else self.source_name

    def walk(self, node, chain, group):
        kids = _structural_kids(node)
        # kit prefix shared by all siblings ("TEOTH_Heads", "TEOTH_Legs_DiscKnees") is ignored
        shared = set.intersection(*(set(N.tokens(c.name)) for c in kids)) if len(kids) >= 3 else set()
        partish = sum(N.is_part_group(' '.join(t for t in N.tokens(c.name) if t not in shared) or c.name)
                      for c in kids)
        if (node in self.force_models or _has_direct(node)
                or (len(kids) > 1 and 2 * partish >= len(kids))):
            self.make_model(node, chain or node, group)
        elif len(kids) == 1:
            self.walk(kids[0], chain or node, group)
        elif kids:
            name_node = chain or node
            g = Group(len(self.groups) + 1, self.name_of(name_node), self.name_of(name_node),
                      node.path(), group, node=node)
            self.groups.append(g)
            node.group = g
            if chain is not None:
                chain.group = g
            for c in kids:
                self.walk(c, None, g)

    def make_model(self, node, name_node, group):
        raw = self.name_of(name_node)
        m = Model(len(self.models) + 1, raw, raw, node.path(), group, node=node)
        self.models.append(m)
        for n in node.walk():
            n.model = m
        model_norm = N.normalize(raw)
        for n in node.walk():
            rel = []
            p = n
            while p is not node:
                if not N.contained_in(N.normalize(p.name), model_norm):
                    rel.append(p.name)
                p = p.parent
            part_group = '/'.join(reversed(rel))
            for f in n.files:
                if f.kind in ('image', 'junk', 'meta'):
                    continue
                m.files.append(ModelFile(f.entry, f.name, f.kind, part_group, N.variant_label(f.variant)))


def _subtree_models(node):
    out = []
    for n in node.walk():
        if n.model is not None and n.model not in out:
            out.append(n.model)
    return out


def _is_below(node, ancestor):
    while node is not None:
        if node is ancestor:
            return True
        node = node.parent
    return False


def _nearest_group(node):
    while node is not None:
        if node.group is not None:
            return node.group
        if node.model is not None and node.model.group is not None:
            return node.model.group
        node = node.parent
    return None


def _match_by_id(fname, models):
    toks = N.normalize(N.stem(fname)).split()
    ids = {t for t in toks if t.isdigit() and len(t) >= 3} | {t.upper() for t in toks if t.lower().startswith('hl')}
    hits = [m for m in models if m.attrs.get('catalog_id') in ids]
    return hits[0] if len(hits) == 1 else None


def _name_tokens(name):
    return [N.singular(t) for t in N.tokens(name) if len(t) >= 3 and not t.isdigit()]


def _match_by_name(fname, models, context=()):
    """context: tokens of the folder the image sits in (and its ancestors). A model token that
    just repeats them ("BUILDER 1" under "CHAOS MARINE BUILDER") is no evidence."""
    raw = N.image_tokens(fname)
    itoks = {N.singular(t) for t in raw}
    if not itoks:
        return None
    # split compounds: "bs-exo-suits-render" should hit the model word "Exosuits"
    hit_toks = itoks | {N.singular(a + b) for a, b in zip(raw, raw[1:])}
    # tokens shared by most candidates ("Battle Sisters - ...") don't identify a model
    counts = Counter(t for m in models for t in set(_name_tokens(m.name)))
    common = {t for t, c in counts.items() if len(models) >= 3 and c > len(models) / 2}
    # the image's own identifying words ("BS Bases 1" -> {base})
    idistinct = {t for t in itoks if len(t) >= 3 and not t.isdigit() and t not in context}
    scored = []
    for m in models:
        own = [t for t in _name_tokens(m.name) if t not in context]
        mtoks = [t for t in own if t not in common] or own
        if not mtoks:
            continue
        hit = sum(t in hit_toks for t in mtoks)
        if (hit == len(mtoks) or (hit >= 2 and hit / len(mtoks) >= 0.6)
                or (hit and idistinct and idistinct <= set(own))):
            scored.append((hit / len(mtoks), hit, m))
    if not scored:
        return None
    scored.sort(key=lambda x: (x[1], x[0]), reverse=True)
    if len(scored) > 1 and scored[0][:2] == scored[1][:2]:
        return None
    return scored[0][2]


def _context_tokens(node):
    context = set()
    while node is not None:
        context.update(_name_tokens(node.name))
        node = node.parent
    return context


def _match_sidecar(fname, node):
    """D1/D4: a picture next to an archive or folder of (nearly) the same name belongs to it:
    'Eternal Eagle (3).jpg' / 'Eternal Eagle.rar', '40K_Empress_fina.jpg' / '40K_Empress_final.rar'."""
    key = N.sidecar_key(fname)
    if len(key) < 4:
        return None
    scored = []
    for c in node.children.values():
        if not c.structural:
            continue
        ck = N.sidecar_key(c.name)
        if ck == key:
            scored.append((3.0, c))
        elif min(len(ck), len(key)) >= 6 and (ck.startswith(key) or key.startswith(ck)):
            scored.append((1 + min(len(ck), len(key)) / 1000, c))
    if not scored:
        return None
    scored.sort(key=lambda x: x[0], reverse=True)
    if len(scored) > 1 and scored[0][0] == scored[1][0]:
        return None
    return scored[0][1]


def _first_group_at_or_below(node):
    for n in node.walk():
        if n.group is not None:
            return n.group
    return None


def _adopt_catalog_ids(models):
    """A model folder without a number ("Bullboyz 1") whose own pictures carry one
    ("442 Complete bullboyz 1.png") takes that number, so group-level images can match by id."""
    for m in models:
        if 'catalog_id' in m.attrs:
            continue
        ids = {N.catalog_id(N.normalize(a.name).removeprefix('resize ')) for a in m.assets if a.kind == 'image'}
        ids = {i for i in ids if i and not (i.isdigit() and 1990 <= int(i) <= 2035)}   # not a year
        if len(ids) == 1:
            m.attrs['catalog_id'] = ids.pop()


def _attach_assets(root, models, groups):
    assets, pending = [], []
    by_part = {}
    for m in models:
        for f in m.files:
            by_part.setdefault(f.name.lower(), m)
    # pass 1: thumbnails and images that sit inside a model
    for node in root.walk():
        for f in node.files:
            if f.kind not in ('image', 'doc'):
                continue
            role = N.image_role(f.name) if f.kind == 'image' else 'doc'
            a = Asset(f.entry, f.name, f.kind, role, generic=f.kind == 'image' and N.is_generic_image(f.name))
            assets.append(a)
            if role == 'thumb' and N.stem(f.name).lower() in by_part:
                a.model, a.match = by_part[N.stem(f.name).lower()], 'thumb'
            elif node.model is not None:
                a.model, a.match = node.model, 'inside'
                p, d = node, 0
                while p is not node.model.node and p is not None:
                    p, d = p.parent, d + 1
                a.depth = d
            else:
                pending.append((a, node))
                continue
            a.model.assets.append(a)
    _adopt_catalog_ids(models)
    # pass 2: images above models -> only child, catalog id, name; else poster of best group
    for a, node in pending:
        side = _match_sidecar(a.name, node) if a.kind == 'image' else None
        if side is not None:
            ms = _subtree_models(side)
            if len(ms) == 1:
                a.model, a.depth, a.match = ms[0], 0, 'sidecar'
                ms[0].assets.append(a)
                continue
            g = _first_group_at_or_below(side)
            if g is not None:
                a.group, a.match = g, 'sidecar'
                if a.role == 'preview':
                    a.role = 'poster'
                continue
        cands = _subtree_models(node)
        context = _context_tokens(node)
        if len(cands) == 1:
            hit, a.match = cands[0], 'only'
        else:
            hit, a.match = _match_by_id(a.name, cands), 'id'
            if hit is None:
                hit, a.match = _match_by_name(a.name, cands, context), 'name'
        if hit is not None:
            a.model, a.depth = hit, 1
            hit.assets.append(a)
            continue
        sub = [g for g in groups if g.node is not None and g.node is not node and _is_below(g.node, node)]
        a.group = _match_by_name(a.name, sub, context) or _nearest_group(node)
        a.match = 'poster'
        if a.role == 'preview':
            a.role = 'poster'
    for m in models:
        m.assets.sort(key=lambda a: (a.kind != 'image', ROLE_ORDER.get(a.role, 9), a.generic,
                                     a.depth, -(a.entry.size or 0) if a.generic else 0, a.name.lower()))
    return assets


def _finalize_names(res, source_name):
    tags = Counter()
    for m in res.models:
        for t in set(N.parenthetical_tags(m.raw_name)):
            # designer handles look like "VirusesofDeath", "HappyDuck3D", not "(Drone)"
            handle = len(t) >= 8 or any(ch.isdigit() or ch.isupper() for ch in t[1:])
            if handle and t.lower() not in ROLE_EXCLUDE_DESIGNER and not t[0].isdigit():
                tags[t] += 1
    designer_tags = [t for t, c in tags.items() if c >= 2]
    for obj in list(res.models) + list(res.groups):
        obj.name, obj.attrs = N.parse_name(obj.raw_name, source_name, designer_tags)
    by_group = {}
    for m in res.models:
        by_group.setdefault(m.group, []).append(m)
    for sibs in by_group.values():
        _strip_shared_prefix(sibs)


def _strip_shared_prefix(models):
    """D6: 'Warhammer 40k - Wargames Terrain - Ancient Alien Pylon' x153 -> 'Ancient Alien Pylon'.
    Strips the longest ' - '-terminated prefix shared by enough siblings (display name only)."""
    if len(models) < 5:
        return
    split = [m.name.split(' ') for m in models]
    counts = Counter()
    for toks in split:
        for k, t in enumerate(toks[:-1]):
            if t == '-':
                counts[tuple(x.lower() for x in toks[:k + 1])] += 1
    need = max(3, -(-len(models) * 4 // 5))              # >= 80% of the siblings
    for m, toks in zip(models, split):
        best = 0
        for k, t in enumerate(toks[:-1]):
            if t == '-' and counts[tuple(x.lower() for x in toks[:k + 1])] >= need:
                best = k + 1
        if best:
            m.name = ' '.join(toks[best:]).strip(' -') or m.name


def _unit_groups(res):
    """Groups whose models have no images of their own while the group has some: the images
    describe the group as a whole (a squad / kit / battalion), so it should be one model.
    Top-down: once a group is chosen, its subgroups are not considered separately."""
    imaged = {m for m in res.models if any(a.kind == 'image' for a in m.assets)}
    posters = {a.group for a in res.assets if a.model is None and a.kind == 'image' and a.group}
    chosen = []
    for g in res.groups:                                 # parents come before children
        p = g.parent
        while p is not None and p not in chosen:
            p = p.parent
        if p is not None or g not in posters:
            continue
        inside = [m for m in res.models if _under(m.group, g)]
        if inside and not any(m in imaged for m in inside):
            chosen.append(g)
    return [g.node for g in chosen]


def _under(group, ancestor):
    while group is not None:
        if group is ancestor:
            return True
        group = group.parent
    return False


def _resolve_once(source_name, lroot, force=()):
    for n in lroot.walk():
        n.model = n.group = None
    r = _Resolver(source_name, force)
    if lroot.structural:
        r.walk(lroot, None, None)
    res = Result(source_name, r.groups, r.models, [])
    _finalize_names(res, source_name)
    res.assets = _attach_assets(lroot, r.models, r.groups)
    return res


def resolve(source_name, entries):
    """entries: iterable of Entry. Returns Result."""
    proot = _build_physical(entries)
    _mark_variants(proot)
    lroot = LNode(source_name)
    _build_logical(proot, lroot, frozenset(), [source_name])
    _mark_logical_structural(lroot)
    res = _resolve_once(source_name, lroot)
    units = _unit_groups(res)
    if units:
        res = _resolve_once(source_name, lroot, units)
    return res
