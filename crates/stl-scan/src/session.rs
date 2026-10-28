//! Extract many files one at a time, keeping the archives on the way to the last one open:
//! files from the same (nested) archive in a row don't reopen it.
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::TempPath;

use crate::archive::{self, Archive, ArchiveInput};
use crate::error::ScanError;
use crate::extract::{CountingWriter, SourceKind};
use crate::nested::{ListOptions, open_member};

struct Level {
    /// The member this archive was opened from; `None` for the source itself.
    member: Option<String>,
    archive: Box<dyn Archive>,
    /// A spilled nested archive must outlive its reader.
    _temp: Option<TempPath>,
}

/// The open stack for one source: the source archive, then one level per nested archive.
#[derive(Default)]
pub struct ExtractSession {
    source: Option<PathBuf>,
    stack: Vec<Level>,
    opens: usize,
    opts: ListOptions,
}

impl ExtractSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Archives opened so far (outer and nested).
    pub fn opens(&self) -> usize {
        self.opens
    }

    /// Like `extract::extract_entry`, reusing the open archives that the chain shares with the
    /// previous call. A failure drops them all (a reader may be left mid-stream).
    pub fn extract(&mut self, kind: SourceKind, source_path: &Path, chain: &[String], out: &mut dyn Write) -> Result<u64, ScanError> {
        let (last, outer) = chain.split_last().ok_or_else(|| ScanError::Format("empty chain".into()))?;
        let mut counter = CountingWriter { inner: out, n: 0 };
        if kind == SourceKind::Folder {
            let mut f = std::fs::File::open(source_path.join(last))?;
            std::io::copy(&mut f, &mut counter)?;
            return Ok(counter.n);
        }
        let r = self.open_to(source_path, outer).and_then(|top| top.extract(last, &mut counter));
        if r.is_err() {
            self.source = None;
            self.stack.clear();
        }
        r.map(|()| counter.n)
    }

    /// The innermost archive of `outer` inside `source_path`, opening only what isn't open yet.
    fn open_to(&mut self, source_path: &Path, outer: &[String]) -> Result<&mut dyn Archive, ScanError> {
        if self.source.as_deref() != Some(source_path) {
            self.source = None;
            self.stack.clear();
            let a = archive::open(ArchiveInput::Path(source_path.to_path_buf()))?;
            self.opens += 1;
            self.stack.push(Level { member: None, archive: a, _temp: None });
            self.source = Some(source_path.to_path_buf());
        }
        let shared = self.stack[1..].iter().zip(outer).take_while(|(l, m)| l.member.as_deref() == Some(m.as_str())).count();
        self.stack.truncate(1 + shared);
        for member in &outer[shared..] {
            let parent = self.stack.last_mut().expect("the source is open");
            let (a, temp) = open_member(parent.archive.as_mut(), member, &self.opts)?;
            self.opens += 1;
            self.stack.push(Level { member: Some(member.clone()), archive: a, _temp: temp });
        }
        Ok(self.stack.last_mut().expect("the source is open").archive.as_mut())
    }
}
