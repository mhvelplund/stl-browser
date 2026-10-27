//! One interface over the archive formats. Every reader lists file members (directories are
//! left out) and extracts them by their raw member name.
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub use crate::error::ScanError;

/// Where an archive's bytes come from. `Bytes` carries a name so the format can be told from
/// its extension.
#[derive(Debug, Clone)]
pub enum ArchiveInput {
    Path(PathBuf),
    Bytes(Arc<Vec<u8>>, String),
}

impl ArchiveInput {
    fn name(&self) -> String {
        match self {
            ArchiveInput::Path(p) => p.to_string_lossy().into_owned(),
            ArchiveInput::Bytes(_, name) => name.clone(),
        }
    }

    fn reader(&self) -> Result<Box<dyn ReadSeek>, ScanError> {
        Ok(match self {
            ArchiveInput::Path(p) => Box::new(BufReader::new(File::open(p)?)),
            ArchiveInput::Bytes(b, _) => Box::new(Cursor::new(SharedBytes(b.clone()))),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name as stored in the archive; pass it back to `extract`.
    pub raw_name: String,
    /// `/`-separated path.
    pub path: String,
    pub size: u64,
    /// 8 uppercase hex digits, or `""` when the format stores none.
    pub crc: String,
}

pub trait Archive: Send {
    fn list(&mut self) -> Result<Vec<Member>, ScanError>;
    fn extract(&mut self, raw_name: &str, out: &mut dyn Write) -> Result<(), ScanError>;
    /// Extract a member to a file. Readers that can only hand out whole members in memory
    /// (RAR) override this to write straight to disk.
    fn extract_to_path(&mut self, raw_name: &str, dest: &std::path::Path) -> Result<(), ScanError> {
        let mut f = std::io::BufWriter::new(File::create(dest)?);
        self.extract(raw_name, &mut f)?;
        f.flush()?;
        Ok(())
    }
    /// True when `extract` buffers the whole member in memory, so big members should go
    /// through `extract_to_path` instead.
    fn buffers_members(&self) -> bool {
        false
    }
    /// Extract several members in one sequential pass (cheap for solid archives). `f` gets each
    /// requested member's raw name and bytes.
    fn extract_many(
        &mut self,
        raw_names: &HashSet<String>,
        f: &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>,
    ) -> Result<(), ScanError>;
}

/// Open an archive, choosing the reader by the lowercase extension of its name.
pub fn open(input: ArchiveInput) -> Result<Box<dyn Archive>, ScanError> {
    let name = input.name();
    match stl_core::names::ext(&name).as_str() {
        ".zip" => Ok(Box::new(ZipReader::new(&input)?)),
        ".7z" => Ok(Box::new(SevenZReader::new(&input)?)),
        ".rar" => Ok(Box::new(RarReader::new(input)?)),
        e => Err(ScanError::Unsupported(format!("archive type '{e}' ({name})"))),
    }
}

pub(crate) fn crc_hex(crc: u32) -> String {
    format!("{crc:08X}")
}

pub(crate) fn normalize_path(raw: &str) -> String {
    raw.replace('\\', "/")
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

struct SharedBytes(Arc<Vec<u8>>);
impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// ---- ZIP ----

struct ZipReader {
    zip: zip::ZipArchive<Box<dyn ReadSeek>>,
    /// raw name -> index; names are decoded once so extraction never depends on lookup rules.
    index: HashMap<String, usize>,
}

fn zip_err(e: zip::result::ZipError) -> ScanError {
    use zip::result::ZipError as Z;
    match e {
        Z::Io(e) => ScanError::Io(e),
        Z::InvalidPassword => ScanError::Encrypted,
        Z::FileNotFound => ScanError::Format("member not found".into()),
        other => ScanError::Format(other.to_string()),
    }
}

impl ZipReader {
    fn new(input: &ArchiveInput) -> Result<Self, ScanError> {
        let mut zip = zip::ZipArchive::new(input.reader()?).map_err(zip_err)?;
        let mut index = HashMap::new();
        for i in 0..zip.len() {
            let f = zip.by_index_raw(i).map_err(zip_err)?;
            index.insert(f.name().map_err(zip_err)?.into_owned(), i);
        }
        Ok(Self { zip, index })
    }

    fn read_index(&mut self, i: usize, out: &mut dyn Write) -> Result<(), ScanError> {
        let mut f = self.zip.by_index(i).map_err(zip_err)?;
        if f.encrypted() {
            return Err(ScanError::Encrypted);
        }
        std::io::copy(&mut f, out)?;
        Ok(())
    }
}

impl Archive for ZipReader {
    fn list(&mut self) -> Result<Vec<Member>, ScanError> {
        let mut out = Vec::new();
        for i in 0..self.zip.len() {
            let f = self.zip.by_index_raw(i).map_err(zip_err)?;
            if f.is_dir() {
                continue;
            }
            let raw = f.name().map_err(zip_err)?.into_owned();
            out.push(Member { path: normalize_path(&raw), raw_name: raw, size: f.size(), crc: crc_hex(f.crc32()) });
        }
        Ok(out)
    }

    fn extract(&mut self, raw_name: &str, out: &mut dyn Write) -> Result<(), ScanError> {
        let i = *self.index.get(raw_name).ok_or_else(|| ScanError::Format(format!("no member '{raw_name}'")))?;
        self.read_index(i, out)
    }

    fn extract_many(
        &mut self,
        raw_names: &HashSet<String>,
        f: &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let mut wanted: Vec<(usize, String)> =
            raw_names.iter().filter_map(|n| self.index.get(n).map(|&i| (i, n.clone()))).collect();
        wanted.sort();
        for (i, name) in wanted {
            let mut buf = Vec::new();
            self.read_index(i, &mut buf)?;
            f(&name, &buf)?;
        }
        Ok(())
    }
}

// ---- 7Z ----

struct SevenZReader {
    reader: sevenz_rust2::ArchiveReader<Box<dyn ReadSeek>>,
}

fn sevenz_err(e: sevenz_rust2::Error) -> ScanError {
    use sevenz_rust2::Error as E;
    match e {
        E::PasswordRequired | E::MaybeBadPassword(_) => ScanError::Encrypted,
        E::Io(e, _) | E::FileOpen(e, _) => ScanError::Io(e),
        E::UnsupportedCompressionMethod(m) => ScanError::Unsupported(format!("7z method {m}")),
        other => ScanError::Format(other.to_string()),
    }
}

impl SevenZReader {
    fn new(input: &ArchiveInput) -> Result<Self, ScanError> {
        let reader = sevenz_rust2::ArchiveReader::new(input.reader()?, sevenz_rust2::Password::empty())
            .map_err(sevenz_err)?;
        Ok(Self { reader })
    }
}

impl Archive for SevenZReader {
    fn list(&mut self) -> Result<Vec<Member>, ScanError> {
        Ok(self
            .reader
            .archive()
            .files
            .iter()
            .filter(|f| !f.is_directory && !f.is_anti_item)
            .map(|f| Member {
                raw_name: f.name.clone(),
                path: normalize_path(&f.name),
                size: f.size,
                crc: if f.has_crc { crc_hex(f.crc as u32) } else { String::new() },
            })
            .collect())
    }

    /// Streams the member: entries before it in a solid block are decoded into a sink, never
    /// buffered (`read_file` would hold them all in memory).
    fn extract(&mut self, raw_name: &str, out: &mut dyn Write) -> Result<(), ScanError> {
        let mut found = false;
        let mut write_err: Option<std::io::Error> = None;
        let r = self.reader.for_each_entries(|entry, data| {
            if entry.name != raw_name || entry.is_directory {
                std::io::copy(data, &mut std::io::sink()).map_err(|e| sevenz_rust2::Error::Io(e, "skip".into()))?;
                return Ok(true);
            }
            found = true;
            if let Err(e) = std::io::copy(data, out) {
                write_err = Some(e);
            }
            Ok(false)
        });
        if let Some(e) = write_err {
            return Err(ScanError::Io(e));
        }
        r.map_err(sevenz_err)?;
        if found { Ok(()) } else { Err(ScanError::Format(format!("no member '{raw_name}'"))) }
    }

    fn extract_many(
        &mut self,
        raw_names: &HashSet<String>,
        f: &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let mut remaining = raw_names.len();
        let mut inner: Option<ScanError> = None;
        let r = self.reader.for_each_entries(|entry, data| {
            if remaining == 0 {
                return Ok(false);
            }
            if entry.is_directory || !raw_names.contains(&entry.name) {
                std::io::copy(data, &mut std::io::sink())
                    .map_err(|e| sevenz_rust2::Error::Io(e, "skip".into()))?;
                return Ok(true);
            }
            // the header's size is untrusted: don't let a corrupt one reserve gigabytes up front
            let mut buf = Vec::with_capacity(entry.size.min(1 << 20) as usize);
            data.read_to_end(&mut buf).map_err(|e| sevenz_rust2::Error::Io(e, "read".into()))?;
            remaining -= 1;
            if let Err(e) = f(&entry.name, &buf) {
                inner = Some(e);
                return Ok(false);
            }
            Ok(true)
        });
        if let Some(e) = inner {
            return Err(e);
        }
        r.map_err(sevenz_err)
    }
}

// ---- RAR ----

/// The unrar library only reads files on disk, so `Bytes` input is written to a temp file first.
/// Every operation reopens the archive and walks its headers in order.
struct RarReader {
    path: PathBuf,
    _temp: Option<tempfile::NamedTempFile>,
}

fn rar_err(e: unrar::error::UnrarError) -> ScanError {
    use unrar::error::Code as C;
    match e.code {
        C::MissingPassword | C::BadPassword => ScanError::Encrypted,
        C::EOpen | C::ERead => ScanError::Io(std::io::Error::other(e.to_string())),
        _ => ScanError::Format(e.to_string()),
    }
}

impl RarReader {
    fn new(input: ArchiveInput) -> Result<Self, ScanError> {
        let (path, temp) = match input {
            ArchiveInput::Path(p) => (p, None),
            ArchiveInput::Bytes(b, _) => {
                let mut t = tempfile::Builder::new().prefix("stl-browser-").suffix(".rar").tempfile()?;
                t.write_all(&b)?;
                t.flush()?;
                (t.path().to_path_buf(), Some(t))
            }
        };
        let r = Self { path, _temp: temp };
        // Fail early (like the other readers) on a missing, corrupt or header-encrypted archive.
        let open = unrar::Archive::new(&r.path).open_for_listing().map_err(rar_err)?;
        if open.has_encrypted_headers() {
            return Err(ScanError::Encrypted);
        }
        Ok(r)
    }

    /// Walk the archive once; `want` picks the entries to unpack and receives their bytes.
    fn walk(&self, want: &mut dyn FnMut(&str) -> bool, got: &mut dyn FnMut(&str, Vec<u8>) -> Result<bool, ScanError>) -> Result<(), ScanError> {
        let mut cursor = unrar::Archive::new(&self.path).open_for_processing().map_err(rar_err)?;
        while let Some(header) = cursor.read_header().map_err(rar_err)? {
            let name = header.entry().filename.to_string_lossy().into_owned();
            if header.entry().is_file() && want(&name) {
                if header.entry().is_encrypted() {
                    return Err(ScanError::Encrypted);
                }
                let (data, next) = header.read().map_err(rar_err)?;
                if !got(&name, data)? {
                    return Ok(());
                }
                cursor = next;
            } else {
                cursor = header.skip().map_err(rar_err)?;
            }
        }
        Ok(())
    }
}

impl Archive for RarReader {
    fn list(&mut self) -> Result<Vec<Member>, ScanError> {
        let mut out = Vec::new();
        for entry in unrar::Archive::new(&self.path).open_for_listing().map_err(rar_err)? {
            let entry = entry.map_err(rar_err)?;
            if !entry.is_file() {
                continue;
            }
            let raw = entry.filename.to_string_lossy().into_owned();
            out.push(Member { path: normalize_path(&raw), raw_name: raw, size: entry.unpacked_size, crc: crc_hex(entry.file_crc) });
        }
        Ok(out)
    }

    fn extract(&mut self, raw_name: &str, out: &mut dyn Write) -> Result<(), ScanError> {
        let mut found = false;
        self.walk(&mut |n| n == raw_name, &mut |_, data| {
            out.write_all(&data)?;
            found = true;
            Ok(false)
        })?;
        if found { Ok(()) } else { Err(ScanError::Format(format!("no member '{raw_name}'"))) }
    }

    fn extract_to_path(&mut self, raw_name: &str, dest: &std::path::Path) -> Result<(), ScanError> {
        let mut cursor = unrar::Archive::new(&self.path).open_for_processing().map_err(rar_err)?;
        while let Some(header) = cursor.read_header().map_err(rar_err)? {
            let entry = header.entry();
            if entry.is_file() && entry.filename.to_string_lossy() == raw_name {
                if entry.is_encrypted() {
                    return Err(ScanError::Encrypted);
                }
                header.extract_to(dest).map_err(rar_err)?;
                return Ok(());
            }
            cursor = header.skip().map_err(rar_err)?;
        }
        Err(ScanError::Format(format!("no member '{raw_name}'")))
    }

    fn buffers_members(&self) -> bool {
        true
    }

    fn extract_many(
        &mut self,
        raw_names: &HashSet<String>,
        f: &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>,
    ) -> Result<(), ScanError> {
        let mut remaining = raw_names.len();
        if remaining == 0 {
            return Ok(());
        }
        self.walk(&mut |n| raw_names.contains(n), &mut |name, data| {
            f(name, &data)?;
            remaining -= 1;
            Ok(remaining > 0)
        })
    }
}
