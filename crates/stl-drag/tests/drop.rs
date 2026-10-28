//! Drop a prepared drag on the shell's own drop target for an empty folder, the way Explorer
//! handles a real drop: it copies the virtual files (asynchronously) into the folder.
#![cfg(windows)]

use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use stl_drag::{Contents, Descriptor, prepare};
use windows::Win32::Foundation::POINTL;
use windows::Win32::System::Ole::{DROPEFFECT_COPY, IDropTarget, OleInitialize, OleUninitialize};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{BHID_SFUIObject, IShellItem, SHCreateItemFromParsingName};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
use windows_core::HSTRING;

struct Bodies {
    bodies: Vec<&'static [u8]>,
    threads: Mutex<Vec<ThreadId>>,
}

impl Contents for Bodies {
    fn write(&self, index: usize, out: &mut dyn std::io::Write) -> Result<(), String> {
        self.threads.lock().unwrap().push(std::thread::current().id());
        out.write_all(self.bodies[index]).map_err(|e| e.to_string())
    }
}

fn d(rel: &str, is_dir: bool, size: Option<u64>) -> Descriptor {
    Descriptor { rel: rel.into(), is_dir, size }
}

#[test]
fn drop_on_shell_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().to_path_buf();
    let big = vec![b'x'; 3 * 1024 * 1024].leak();
    let contents = Arc::new(Bodies { bodies: vec![b"", b"solid a", b"", b"jpeg b", big], threads: Mutex::new(Vec::new()) });
    let items = vec![
        d("m", true, None),
        d("m\\a.stl", false, Some(7)),
        d("m\\sub", true, None),
        d("m\\sub\\b.jpg", false, Some(6)),
        d("m\\big.stl", false, Some(3 * 1024 * 1024)),
    ];
    let prepared = prepare(items, contents.clone()).unwrap();
    assert_eq!(prepared.too_long, 0);
    let sta = std::thread::spawn(move || unsafe {
        OleInitialize(None).unwrap();
        let obj = prepared.data_object().unwrap();
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(dest.as_os_str()), None).unwrap();
        let target: IDropTarget = item.BindToHandler(None, &BHID_SFUIObject).unwrap();
        let mut effect = DROPEFFECT_COPY;
        target.DragEnter(&obj, MODIFIERKEYS_FLAGS(MK_LBUTTON.0), POINTL::default(), &mut effect).unwrap();
        assert_eq!(effect, DROPEFFECT_COPY);
        let mut effect = DROPEFFECT_COPY;
        target.Drop(&obj, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut effect).unwrap();
        drop((obj, target, item));
        // the copy runs on the shell's thread; keep this STA responsive until the files are there
        let start = Instant::now();
        let len = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        while !(len(&dest.join("m/big.stl")) == 3 * 1024 * 1024 && dest.join("m/sub/b.jpg").is_file()) && start.elapsed() < Duration::from_secs(60) {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        OleUninitialize();
        std::thread::current().id()
    });
    let sta_id = sta.join().unwrap();
    let p = tmp.path();
    assert_eq!(std::fs::read(p.join("m/a.stl")).unwrap(), b"solid a");
    assert_eq!(std::fs::read(p.join("m/sub/b.jpg")).unwrap(), b"jpeg b");
    assert_eq!(std::fs::read(p.join("m/big.stl")).unwrap().len(), 3 * 1024 * 1024);
    let threads = contents.threads.lock().unwrap();
    assert_eq!(threads.len(), 3, "each file is written once");
    assert!(!threads.contains(&sta_id), "contents must not be written on the STA");
}
