//! The data object: file descriptors, one stream per file written on request, async copy.
use std::mem::ManuallyDrop;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{DV_E_FORMATETC, DV_E_LINDEX, E_FAIL, E_NOTIMPL, GlobalFree, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, S_OK};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{
    DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IBindCtx, IDataObject, IDataObject_Impl, IEnumFORMATETC,
    IEnumSTATDATA, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::DROPEFFECT_COPY;
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_PREFERREDDROPEFFECT, FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI,
    FILEDESCRIPTORW, FILEGROUPDESCRIPTORW, IDataObjectAsyncCapability, IDataObjectAsyncCapability_Impl, SHCreateStdEnumFmtEtc,
};
use windows::Win32::Foundation::DATA_S_SAMEFORMATETC;
use windows_core::{BOOL, Error, HRESULT, PCWSTR, Ref, Result, implement};

use crate::{Contents, Descriptor};

const NAME_MAX: usize = 259; // `cFileName` holds 260 UTF-16 units with the terminator

/// Whether `rel` fits a file descriptor.
pub(crate) fn fits(rel: &str) -> bool {
    rel.encode_utf16().count() <= NAME_MAX
}

#[implement(IDataObject, IDataObjectAsyncCapability)]
pub(crate) struct DataObject {
    /// `(index given to Contents, descriptor)`, in descriptor order.
    items: Vec<(usize, Descriptor)>,
    contents: Arc<dyn Contents>,
    cf_descriptor: u16,
    cf_contents: u16,
    cf_effect: u16,
    async_mode: AtomicBool,
    in_operation: AtomicBool,
}

impl DataObject {
    pub(crate) fn create(items: Vec<(usize, Descriptor)>, contents: Arc<dyn Contents>) -> IDataObject {
        let cf = |name: PCWSTR| unsafe { RegisterClipboardFormatW(name) as u16 };
        DataObject {
            items,
            contents,
            cf_descriptor: cf(CFSTR_FILEDESCRIPTORW),
            cf_contents: cf(CFSTR_FILECONTENTS),
            cf_effect: cf(CFSTR_PREFERREDDROPEFFECT),
            async_mode: AtomicBool::new(true),
            in_operation: AtomicBool::new(false),
        }
        .into()
    }

    fn formats(&self) -> [FORMATETC; 3] {
        let f = |cf: u16, tymed: i32| FORMATETC { cfFormat: cf, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: tymed as u32 };
        [f(self.cf_descriptor, TYMED_HGLOBAL.0), f(self.cf_contents, TYMED_ISTREAM.0), f(self.cf_effect, TYMED_HGLOBAL.0)]
    }

    fn descriptors(&self) -> Result<HGLOBAL> {
        let n = self.items.len();
        let size = std::mem::offset_of!(FILEGROUPDESCRIPTORW, fgd) + n.max(1) * std::mem::size_of::<FILEDESCRIPTORW>();
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size)?;
            let p = GlobalLock(h) as *mut FILEGROUPDESCRIPTORW;
            if p.is_null() {
                let _ = GlobalFree(Some(h));
                return Err(Error::from_thread());
            }
            std::ptr::addr_of_mut!((*p).cItems).write_unaligned(n as u32);
            let first = std::ptr::addr_of_mut!((*p).fgd) as *mut FILEDESCRIPTORW;
            for (i, (_, d)) in self.items.iter().enumerate() {
                // FILEDESCRIPTORW is packed: build it here and copy it in unaligned
                let mut fd: FILEDESCRIPTORW = std::mem::zeroed();
                let mut flags = FD_ATTRIBUTES.0 as u32 | FD_PROGRESSUI.0 as u32;
                fd.dwFileAttributes = if d.is_dir { FILE_ATTRIBUTE_DIRECTORY.0 } else { FILE_ATTRIBUTE_NORMAL.0 };
                if let (false, Some(size)) = (d.is_dir, d.size) {
                    flags |= FD_FILESIZE.0 as u32;
                    fd.nFileSizeHigh = (size >> 32) as u32;
                    fd.nFileSizeLow = size as u32;
                }
                fd.dwFlags = flags;
                let mut name = [0u16; 260];
                for (k, c) in d.rel.encode_utf16().take(NAME_MAX).enumerate() {
                    name[k] = c;
                }
                fd.cFileName = name;
                first.add(i).write_unaligned(fd);
            }
            let _ = GlobalUnlock(h);
            Ok(h)
        }
    }

    fn contents_stream(&self, lindex: i32) -> Result<STGMEDIUM> {
        let (index, d) = usize::try_from(lindex).ok().and_then(|i| self.items.get(i)).ok_or(Error::from(DV_E_LINDEX))?;
        if d.is_dir {
            return Err(DV_E_LINDEX.into());
        }
        // Explorer copies exactly the declared size, so an unknown or too small size would
        // silently cut the file short: fail instead
        let limit = d.size.ok_or(Error::from(E_FAIL))?;
        let contents = self.contents.clone();
        let index = *index;
        let r = catch_unwind(AssertUnwindSafe(|| super::stream::temp_stream(limit, |f| contents.write(index, f))));
        match r {
            Ok(Ok(s)) => Ok(STGMEDIUM { tymed: TYMED_ISTREAM.0 as u32, u: STGMEDIUM_0 { pstm: ManuallyDrop::new(Some(s)) }, pUnkForRelease: ManuallyDrop::new(None) }),
            _ => Err(E_FAIL.into()),
        }
    }
}

fn hglobal_u32(v: u32) -> Result<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, 4)?;
        let p = GlobalLock(h) as *mut u32;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(Error::from_thread());
        }
        p.write_unaligned(v);
        let _ = GlobalUnlock(h);
        Ok(h)
    }
}

fn hglobal_medium(h: HGLOBAL) -> STGMEDIUM {
    STGMEDIUM { tymed: TYMED_HGLOBAL.0 as u32, u: STGMEDIUM_0 { hGlobal: h }, pUnkForRelease: ManuallyDrop::new(None) }
}

impl IDataObject_Impl for DataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        let f = unsafe { pformatetcin.as_ref() }.ok_or(Error::from(DV_E_FORMATETC))?;
        let has = |t: i32| f.tymed & t as u32 != 0;
        if f.cfFormat == self.cf_descriptor && has(TYMED_HGLOBAL.0) {
            return Ok(hglobal_medium(self.descriptors()?));
        }
        if f.cfFormat == self.cf_contents && has(TYMED_ISTREAM.0) {
            return self.contents_stream(f.lindex);
        }
        if f.cfFormat == self.cf_effect && has(TYMED_HGLOBAL.0) {
            return Ok(hglobal_medium(hglobal_u32(DROPEFFECT_COPY.0)?));
        }
        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        let Some(f) = (unsafe { pformatetc.as_ref() }) else { return DV_E_FORMATETC };
        if self.formats().iter().any(|o| o.cfFormat == f.cfFormat && o.tymed & f.tymed != 0) { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, pformatetcout: *mut FORMATETC) -> HRESULT {
        if let Some(out) = unsafe { pformatetcout.as_mut() } {
            out.ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        unsafe { SHCreateStdEnumFmtEtc(&self.formats()) }
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl IDataObjectAsyncCapability_Impl for DataObject_Impl {
    fn SetAsyncMode(&self, on: BOOL) -> Result<()> {
        self.async_mode.store(on.as_bool(), Ordering::SeqCst);
        Ok(())
    }

    fn GetAsyncMode(&self) -> Result<BOOL> {
        Ok(self.async_mode.load(Ordering::SeqCst).into())
    }

    fn StartOperation(&self, _: Ref<IBindCtx>) -> Result<()> {
        self.in_operation.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> Result<BOOL> {
        Ok(self.in_operation.load(Ordering::SeqCst).into())
    }

    fn EndOperation(&self, _: HRESULT, _: Ref<IBindCtx>, _: u32) -> Result<()> {
        self.in_operation.store(false, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::IStream;
    use windows::Win32::System::Ole::ReleaseStgMedium;

    struct Fixed;

    impl Contents for Fixed {
        fn write(&self, index: usize, out: &mut dyn std::io::Write) -> std::result::Result<(), String> {
            match index {
                1 => Err("broken member".into()),
                5 => panic!("reader blew up"),
                i => out.write_all(format!("file {i}").as_bytes()).map_err(|e| e.to_string()),
            }
        }
    }

    fn d(rel: &str, is_dir: bool, size: Option<u64>) -> Descriptor {
        Descriptor { rel: rel.into(), is_dir, size }
    }

    fn object(items: Vec<Descriptor>) -> IDataObject {
        DataObject::create(items.into_iter().enumerate().collect(), Arc::new(Fixed))
    }

    fn fmt(obj: &IDataObject, name: PCWSTR, tymed: i32, lindex: i32) -> FORMATETC {
        let _ = obj;
        FORMATETC { cfFormat: unsafe { RegisterClipboardFormatW(name) as u16 }, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex, tymed: tymed as u32 }
    }

    fn read_stream(s: &IStream) -> Vec<u8> {
        let mut buf = vec![0u8; 1024];
        let mut n = 0u32;
        let _ = unsafe { s.Read(buf.as_mut_ptr() as _, 1024, Some(&mut n)) };
        buf.truncate(n as usize);
        buf
    }

    fn contents(obj: &IDataObject, i: i32) -> Result<Vec<u8>> {
        let mut m = unsafe { obj.GetData(&fmt(obj, CFSTR_FILECONTENTS, TYMED_ISTREAM.0, i))? };
        let bytes = read_stream(unsafe { m.u.pstm.as_ref().unwrap() });
        unsafe { ReleaseStgMedium(&mut m) };
        Ok(bytes)
    }

    #[test]
    fn descriptor_block_layout() {
        let obj = object(vec![d("m", true, None), d("m\\a.stl", false, Some(5_000_000_000)), d("m\\b.jpg", false, None)]);
        unsafe {
            let mut m = obj.GetData(&fmt(&obj, CFSTR_FILEDESCRIPTORW, TYMED_HGLOBAL.0, -1)).unwrap();
            let p = GlobalLock(m.u.hGlobal) as *const FILEGROUPDESCRIPTORW;
            assert_eq!(std::ptr::addr_of!((*p).cItems).read_unaligned(), 3);
            let first = std::ptr::addr_of!((*p).fgd) as *const FILEDESCRIPTORW;
            let fds: Vec<FILEDESCRIPTORW> = (0..3).map(|i| first.add(i).read_unaligned()).collect();
            let name = |fd: &FILEDESCRIPTORW| {
                let n = fd.cFileName;
                String::from_utf16_lossy(&n[..n.iter().position(|&c| c == 0).unwrap()])
            };
            assert_eq!(fds.iter().map(name).collect::<Vec<_>>(), ["m", "m\\a.stl", "m\\b.jpg"]);
            assert_eq!({ fds[0].dwFileAttributes }, FILE_ATTRIBUTE_DIRECTORY.0);
            assert_eq!({ fds[0].dwFlags } & FD_FILESIZE.0 as u32, 0);
            assert_eq!({ fds[1].dwFlags } & FD_FILESIZE.0 as u32, FD_FILESIZE.0 as u32);
            assert_eq!(({ fds[1].nFileSizeHigh } as u64) << 32 | { fds[1].nFileSizeLow } as u64, 5_000_000_000);
            assert_eq!({ fds[2].dwFlags } & FD_FILESIZE.0 as u32, 0);
            assert_ne!({ fds[2].dwFlags } & FD_PROGRESSUI.0 as u32, 0);
            let _ = GlobalUnlock(m.u.hGlobal);
            ReleaseStgMedium(&mut m);
        }
    }

    #[test]
    fn failed_file_returns_e_fail() {
        let items = (0..6).map(|i| d(&format!("f{i}"), false, Some(6))).collect();
        let obj = object(items);
        assert_eq!(contents(&obj, 0).unwrap(), b"file 0");
        assert_eq!(contents(&obj, 1).unwrap_err().code(), E_FAIL);
        assert_eq!(contents(&obj, 5).unwrap_err().code(), E_FAIL); // a panic, caught
        assert_eq!(contents(&obj, 2).unwrap(), b"file 2");
        assert_eq!(contents(&obj, 9).unwrap_err().code(), DV_E_LINDEX);
    }

    #[test]
    fn offers_three_formats() {
        let obj = object(vec![d("a", false, None)]);
        unsafe {
            assert_eq!(obj.QueryGetData(&fmt(&obj, CFSTR_FILEDESCRIPTORW, TYMED_HGLOBAL.0, -1)), S_OK);
            assert_eq!(obj.QueryGetData(&fmt(&obj, CFSTR_FILECONTENTS, TYMED_ISTREAM.0, 0)), S_OK);
            assert_eq!(obj.QueryGetData(&fmt(&obj, CFSTR_FILECONTENTS, TYMED_HGLOBAL.0, 0)), DV_E_FORMATETC);
            let e = obj.EnumFormatEtc(DATADIR_GET.0 as u32).unwrap();
            let mut got = [FORMATETC::default(); 4];
            let mut n = 0u32;
            let _ = e.Next(&mut got, Some(&mut n));
            assert_eq!(n, 3);
        }
    }

    #[test]
    fn file_longer_than_declared_fails() {
        // Explorer copies only the declared size: a longer file must fail, not be cut short
        let obj = DataObject::create(
            vec![(0, d("a", false, Some(6))), (2, d("b", false, Some(5))), (3, d("c", false, None))],
            Arc::new(Fixed),
        );
        assert_eq!(contents(&obj, 0).unwrap(), b"file 0");
        assert_eq!(contents(&obj, 1).unwrap_err().code(), E_FAIL);
        assert_eq!(contents(&obj, 2).unwrap_err().code(), E_FAIL); // no size: Explorer would truncate
    }

    #[test]
    fn fits_counts_utf16_units() {
        assert!(fits(&"a".repeat(259)));
        assert!(!fits(&"a".repeat(260)));
    }
}
