//! A read-only `IStream` over a temp file that Windows deletes when the last handle closes, so
//! a file of any size costs no memory and leaves nothing behind, even after a crash.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use windows::Win32::Foundation::{E_NOTIMPL, S_FALSE, S_OK, STG_E_ACCESSDENIED, STG_E_INVALIDFUNCTION};
use windows::Win32::System::Com::{
    ISequentialStream_Impl, IStream, IStream_Impl, LOCKTYPE, STATFLAG, STATSTG, STGC, STGTY_STREAM, STREAM_SEEK,
    STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET,
};
use windows_core::{HRESULT, Ref, Result, implement};

#[implement(IStream)]
struct FileStream {
    file: Mutex<File>,
    size: u64,
}

/// Fill a delete-on-close temp file with `write` and return it as a stream at offset 0; more
/// than `limit` bytes is an error.
pub(crate) fn temp_stream(limit: u64, write: impl FnOnce(&mut File) -> std::result::Result<(), String>) -> std::result::Result<IStream, String> {
    let mut file = tempfile::tempfile().map_err(|e| e.to_string())?;
    write(&mut file)?;
    let size = file.stream_position().map_err(|e| e.to_string())?;
    if size > limit {
        return Err(format!("the file has {size} bytes, more than the {limit} in the index"));
    }
    file.rewind().map_err(|e| e.to_string())?;
    Ok(FileStream { file: Mutex::new(file), size }.into())
}

impl ISequentialStream_Impl for FileStream_Impl {
    fn Read(&self, pv: *mut core::ffi::c_void, cb: u32, pcbread: *mut u32) -> HRESULT {
        let buf = unsafe { std::slice::from_raw_parts_mut(pv as *mut u8, cb as usize) };
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let mut n = 0;
        while n < buf.len() {
            match file.read(&mut buf[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return HRESULT::from_win32(e.raw_os_error().unwrap_or(0) as u32),
            }
        }
        if !pcbread.is_null() {
            unsafe { *pcbread = n as u32 };
        }
        if n < cb as usize { S_FALSE } else { S_OK }
    }

    fn Write(&self, _: *const core::ffi::c_void, _: u32, _: *mut u32) -> HRESULT {
        STG_E_ACCESSDENIED
    }
}

impl IStream_Impl for FileStream_Impl {
    fn Seek(&self, mv: i64, origin: STREAM_SEEK, newpos: *mut u64) -> Result<()> {
        let to = match origin {
            STREAM_SEEK_SET => SeekFrom::Start(u64::try_from(mv).map_err(|_| windows_core::Error::from(STG_E_INVALIDFUNCTION))?),
            STREAM_SEEK_CUR => SeekFrom::Current(mv),
            STREAM_SEEK_END => SeekFrom::End(mv),
            _ => return Err(STG_E_INVALIDFUNCTION.into()),
        };
        let pos = self.file.lock().unwrap_or_else(|e| e.into_inner()).seek(to).map_err(|_| windows_core::Error::from(STG_E_INVALIDFUNCTION))?;
        if !newpos.is_null() {
            unsafe { *newpos = pos };
        }
        Ok(())
    }

    fn SetSize(&self, _: u64) -> Result<()> {
        Err(STG_E_ACCESSDENIED.into())
    }

    fn CopyTo(&self, pstm: Ref<IStream>, cb: u64, pcbread: *mut u64, pcbwritten: *mut u64) -> Result<()> {
        let dest = pstm.ok()?;
        let (mut read, mut written) = (0u64, 0u64);
        let mut buf = vec![0u8; 256 * 1024];
        while read < cb {
            let want = (cb - read).min(buf.len() as u64) as usize;
            let n = self.file.lock().unwrap_or_else(|e| e.into_inner()).read(&mut buf[..want]).map_err(|_| windows_core::Error::from(STG_E_INVALIDFUNCTION))?;
            if n == 0 {
                break;
            }
            read += n as u64;
            let mut w = 0u32;
            unsafe { dest.Write(buf.as_ptr() as _, n as u32, Some(&mut w)).ok()? };
            written += w as u64;
        }
        unsafe {
            if !pcbread.is_null() {
                *pcbread = read;
            }
            if !pcbwritten.is_null() {
                *pcbwritten = written;
            }
        }
        Ok(())
    }

    fn Commit(&self, _: &STGC) -> Result<()> {
        Ok(())
    }

    fn Revert(&self) -> Result<()> {
        Ok(())
    }

    fn LockRegion(&self, _: u64, _: u64, _: &LOCKTYPE) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn UnlockRegion(&self, _: u64, _: u64, _: u32) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Stat(&self, pstatstg: *mut STATSTG, _: &STATFLAG) -> Result<()> {
        // no name: callers that ask for one get a null pointer, which is allowed
        unsafe { *pstatstg = STATSTG { r#type: STGTY_STREAM.0 as u32, cbSize: self.size, ..Default::default() } };
        Ok(())
    }

    fn Clone(&self) -> Result<IStream> {
        Err(E_NOTIMPL.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_reads_back_and_stats() {
        let s = temp_stream(11, |f| std::io::Write::write_all(f, b"hello world").map_err(|e| e.to_string())).unwrap();
        let mut buf = [0u8; 32];
        let mut n = 0u32;
        let hr = unsafe { s.Read(buf.as_mut_ptr() as _, 32, Some(&mut n)) };
        assert_eq!(hr, S_FALSE);
        assert_eq!(&buf[..n as usize], b"hello world");
        let mut st = STATSTG::default();
        unsafe { s.Stat(&mut st, STATFLAG(1)).unwrap() };
        assert_eq!(st.cbSize, 11);
        let mut pos = 0u64;
        unsafe { s.Seek(6, STREAM_SEEK_SET, Some(&mut pos)).unwrap() };
        let hr = unsafe { s.Read(buf.as_mut_ptr() as _, 5, Some(&mut n)) };
        assert_eq!((hr, &buf[..n as usize]), (S_OK, &b"world"[..]));
    }
}
