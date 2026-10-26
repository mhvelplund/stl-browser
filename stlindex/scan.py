"""Enumerate sources (archives and loose folders) and list their contents with 7-Zip, recursing
into nested archives. Nothing is extracted except nested archives, into a temp dir, for listing."""
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass, field

from . import names as N

_7Z_CANDIDATES = [r'C:\Program Files\7-Zip\7z.exe', r'C:\Program Files (x86)\7-Zip\7z.exe',
                  '/usr/bin/7z', '/usr/local/bin/7z', '/opt/homebrew/bin/7z']
# dummy password: never block on a prompt; encrypted entries just fail to extract
_NO_PROMPT = '-pstlindex-no-password'


class ScanError(Exception):
    pass


def find_7z(explicit=None):
    for c in [explicit, shutil.which('7z'), shutil.which('7zz'), shutil.which('7za'), *_7Z_CANDIDATES]:
        if c and os.path.isfile(c):
            return c
    raise FileNotFoundError('7-Zip not found; install it or pass --7z PATH')


@dataclass
class SourceInfo:
    kind: str            # archive | folder
    path: str            # absolute path of archive file / folder
    rel_path: str        # "<root name>/<path relative to root>"
    name: str            # display name (archive stem / folder name)
    collection: str      # "<root name>/<top-level folder>" (or just the root name)
    coll_path: str = ''  # path inside the collection folder: archive file path, '' for a folder source
    size: int = 0
    mtime: float = 0.0
    loose_files: list = field(default_factory=list)   # folder sources: relative paths


@dataclass
class ScannedEntry:
    path: str            # '/'-separated logical path, nested archives as segments
    chain: list          # raw member names to extract, outermost first
    size: int
    crc: str = ''


class SevenZip:
    def __init__(self, exe):
        self.exe = exe

    def _run(self, args, **kw):
        return subprocess.run([self.exe, *args], stdin=subprocess.DEVNULL, **kw)

    def list(self, archive):
        p = self._run(['l', '-ba', '-slt', '-sccUTF-8', '-scsUTF-8', _NO_PROMPT, '--', archive],
                      capture_output=True)
        if p.returncode not in (0, 1):
            raise ScanError(p.stderr.decode('utf-8', 'replace').strip() or f'7z exit {p.returncode}')
        out, cur = [], {}
        for line in p.stdout.decode('utf-8', 'replace').splitlines() + ['']:
            if not line.strip():
                if 'Path' in cur:
                    out.append(cur)
                cur = {}
                continue
            k, sep, v = line.partition(' = ')
            if sep:
                cur[k] = v
            elif line.endswith(' ='):
                cur[line[:-2]] = ''
        items = []
        for c in out:
            if c.get('Folder') == '+' or c.get('Attributes', '').startswith('D'):
                continue
            raw = c['Path']
            items.append(ScannedEntry(raw.replace('\\', '/'), [raw], int(c.get('Size') or 0), c.get('CRC', '')))
        return items

    def extract_to(self, archive, member, dest):
        with open(dest, 'wb') as fh:
            p = self._run(['e', '-so', '-spd', _NO_PROMPT, '--', archive, member],
                          stdout=fh, stderr=subprocess.PIPE)
        if p.returncode != 0 or os.path.getsize(dest) == 0:
            raise ScanError(p.stderr.decode('utf-8', 'replace').strip() or f'cannot extract {member}')


def discover(root):
    """Yield SourceInfo for every archive under root, plus one folder source per top-level
    directory that holds loose (non-archive) files."""
    root = os.path.abspath(root)
    vol = os.path.basename(root.rstrip('\\/')) or root
    loose = {}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if not d.startswith('.'))   # app metadata dirs
        rel_dir = os.path.relpath(dirpath, root)
        rel_dir = '' if rel_dir == '.' else rel_dir.replace('\\', '/')
        for fn in sorted(filenames):
            full = os.path.join(dirpath, fn)
            rel = f'{rel_dir}/{fn}' if rel_dir else fn
            top = rel.split('/')[0] if '/' in rel else ''
            kind = N.kind_of(fn)
            if kind == 'archive':
                if N.ext(fn) == '.rar' and _is_secondary_rar_volume(fn):
                    continue
                st = os.stat(full)
                yield SourceInfo('archive', full, f'{vol}/{rel}', _archive_stem(fn), _collection(vol, top),
                                 rel[len(top) + 1:] if top else rel, st.st_size, st.st_mtime)
            elif kind != 'junk':
                loose.setdefault(top, []).append(rel)
    for top, files in sorted(loose.items()):
        path = os.path.join(root, top) if top else root
        inner = [f[len(top) + 1:] if top else f for f in files]
        size = sum(os.path.getsize(os.path.join(path, f)) for f in inner)
        mtime = max(os.path.getmtime(os.path.join(path, f)) for f in inner)
        yield SourceInfo('folder', path, f'{vol}/{top}' if top else vol, top or vol, _collection(vol, top),
                         '', size, mtime, inner)


def _collection(vol, top):
    return f'{vol}/{top}' if top else vol


def _archive_stem(fn):
    s = N.stem(fn)
    for suffix in ('.part1', '.part01', '.part001'):
        if s.lower().endswith(suffix):
            s = s[:-len(suffix)]
    return s


def _is_secondary_rar_volume(fn):
    import re
    m = re.search(r'\.part0*(\d+)\.rar$', fn, re.I)
    return bool(m) and int(m.group(1)) != 1


def scan_source(sz, src, nested=True, warn=print):
    """Return the list of ScannedEntry for a source."""
    if src.kind == 'folder':
        return [ScannedEntry(f, [f], os.path.getsize(os.path.join(src.path, f))) for f in src.loose_files]
    with tempfile.TemporaryDirectory(prefix='stlindex-') as tmp:
        return _scan_archive(sz, src.path, nested, 0, tmp, warn)


def _scan_archive(sz, archive, nested, depth, tmp, warn):
    items = sz.list(archive)
    out = list(items)
    if not nested or depth >= 3:
        return out
    for i, it in enumerate(items):
        if N.kind_of(it.path) != 'archive':
            continue
        dest = os.path.join(tmp, f'd{depth}_{i}{N.ext(it.path)}')
        try:
            sz.extract_to(archive, it.chain[0], dest)
            for sub in _scan_archive(sz, dest, nested, depth + 1, tmp, warn):
                out.append(ScannedEntry(f'{it.path}/{sub.path}', it.chain + sub.chain, sub.size, sub.crc))
        except ScanError as e:
            warn(f'  ! nested {it.path}: {e}')
        finally:
            if os.path.exists(dest):
                os.remove(dest)
    return out


def extract_entry(sz, source_kind, source_path, chain, dest):
    """Extract one indexed file (following the nested-archive chain) to dest. Returns dest."""
    if source_kind == 'folder':
        shutil.copyfile(os.path.join(source_path, chain[0]), dest)
        return dest
    with tempfile.TemporaryDirectory(prefix='stlindex-x-') as tmp:
        current = source_path
        for depth, member in enumerate(chain):
            last = depth == len(chain) - 1
            target = dest if last else os.path.join(tmp, f'n{depth}{N.ext(member)}')
            sz.extract_to(current, member, target)
            current = target
    return dest
