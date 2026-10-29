//! Native OLE drag and drop for the shelf.
//!
//! Drag-out builds the same shell data object Explorer uses (CF_HDROP, shell ID
//! lists, ...) and runs `SHDoDragDrop` on the UI thread, so any app that accepts
//! file drops from Explorer accepts them from WNTR. Text cards drag out as
//! CF_UNICODETEXT.
//!
//! Drag-in replaces WebView2's drop target with ours so we get real file paths,
//! virtual files (images dragged out of browsers) and text.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HANDLE, HGLOBAL, HWND, LPARAM, POINT, POINTL};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL, TYMED_ISTREAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE, IDropSource, IDropSource_Impl, IDropTarget,
    IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    DROPFILES, DragQueryFileW, HDROP, ILClone, ILCreateFromPathW, ILFindLastID, ILFree, ILRemoveLastID,
    SHCreateDataObject, SHDoDragDrop,
};
use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;
use windows::core::{BOOL, HRESULT, PCWSTR, Ref, implement};

use crate::util::{CmdResult, err, wide};

static DRAG_OUT: AtomicBool = AtomicBool::new(false);

pub fn drag_out_active() -> bool {
    DRAG_OUT.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// HGLOBAL helpers
// ---------------------------------------------------------------------------

fn hglobal_from_bytes(bytes: &[u8]) -> windows::core::Result<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1))?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        Ok(h)
    }
}

fn hglobal_bytes(h: HGLOBAL) -> Vec<u8> {
    unsafe {
        let size = GlobalSize(h);
        let p = GlobalLock(h) as *const u8;
        if p.is_null() || size == 0 {
            return Vec::new();
        }
        let v = std::slice::from_raw_parts(p, size).to_vec();
        let _ = GlobalUnlock(h);
        v
    }
}

fn utf16_bytes(s: &str) -> Vec<u8> {
    wide(s).iter().flat_map(|c| c.to_le_bytes()).collect()
}

/// DROPFILES header followed by a double-null-terminated list of wide paths.
fn dropfiles_bytes(paths: &[PathBuf]) -> Vec<u8> {
    let header = std::mem::size_of::<DROPFILES>();
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.as_os_str().to_string_lossy().encode_utf16());
        list.push(0);
    }
    list.push(0);
    let df = DROPFILES {
        pFiles: header as u32,
        pt: POINT::default(),
        fNC: BOOL(0),
        fWide: BOOL(1),
    };
    let mut out = vec![0u8; header];
    unsafe {
        std::ptr::copy_nonoverlapping(&df as *const DROPFILES as *const u8, out.as_mut_ptr(), header);
    }
    out.extend(list.iter().flat_map(|c| c.to_le_bytes()));
    out
}

fn fmt(cf: u16, tymed: u32, lindex: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: cf,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex,
        tymed,
    }
}

fn register_format(name: &str) -> u16 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) as u16 }
}

// ---------------------------------------------------------------------------
// Drag-out
// ---------------------------------------------------------------------------

struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ILFree(Some(self.0)) };
        }
    }
}

/// Shell data object for a set of files, exactly as Explorer would build it.
fn files_data_object(paths: &[PathBuf]) -> windows::core::Result<IDataObject> {
    let abs: Vec<Pidl> = paths
        .iter()
        .map(|p| {
            let w = wide(&p.to_string_lossy());
            Pidl(unsafe { ILCreateFromPathW(PCWSTR(w.as_ptr())) })
        })
        .collect();
    if abs.iter().any(|p| p.0.is_null()) {
        return Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_INVALIDARG,
        ));
    }
    let same_parent = paths
        .iter()
        .all(|p| p.parent().is_some() && p.parent() == paths[0].parent());
    unsafe {
        if same_parent {
            // Canonical form: parent folder + child IDs relative to it.
            let parent = Pidl(ILClone(abs[0].0));
            let _ = ILRemoveLastID(Some(parent.0));
            let children: Vec<*const ITEMIDLIST> = abs.iter().map(|p| ILFindLastID(p.0) as *const ITEMIDLIST).collect();
            if let Ok(obj) = SHCreateDataObject(Some(parent.0), Some(&children), None) {
                return Ok(obj);
            }
        }
        // Files from different folders: a shell object rooted at the desktop yields an
        // empty file list, so carry CF_HDROP ourselves (what every drop target reads).
        let obj: IDataObject = SHCreateDataObject(None, None, None)?;
        set_hglobal(&obj, CF_HDROP.0, &dropfiles_bytes(paths))?;
        Ok(obj)
    }
}

fn set_hglobal(obj: &IDataObject, cf: u16, bytes: &[u8]) -> windows::core::Result<()> {
    let h = hglobal_from_bytes(bytes)?;
    let f = fmt(cf, TYMED_HGLOBAL.0 as u32, -1);
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: windows::Win32::System::Com::STGMEDIUM_0 { hGlobal: h },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    unsafe { obj.SetData(&f, &medium, true) }
}

fn text_data_object(text: &str) -> windows::core::Result<IDataObject> {
    let obj: IDataObject = unsafe { SHCreateDataObject(None, None, None)? };
    set_hglobal(&obj, CF_UNICODETEXT.0, &utf16_bytes(text))?;
    Ok(obj)
}

pub enum Payload {
    Files(Vec<PathBuf>),
    Text(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragResult {
    pub effect: String,
}

/// Ends the drag when the mouse button is released, or cancels it on Escape.
#[implement(IDropSource)]
struct DragSource;

impl IDropSource_Impl for DragSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if grfkeystate.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            HRESULT(0)
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Runs a modal OLE drag on the UI thread. Must be called while the mouse
/// button is still held (the UI calls it from `pointermove`).
pub async fn drag_out(app: &AppHandle, payload: Payload) -> CmdResult<DragResult> {
    let win = crate::notch::window(app).ok_or("notch window missing")?;
    let hwnd = win.hwnd().map_err(err)?.0 as isize;
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<u32, String>>();
    app.run_on_main_thread(move || {
        DRAG_OUT.store(true, Ordering::Relaxed);
        let res = (|| -> windows::core::Result<u32> {
            unsafe {
                let _ = OleInitialize(None);
            }
            let obj = match &payload {
                Payload::Files(p) => {
                    let obj = files_data_object(p)?;
                    // Default to copy even on the same drive (Shift still moves), so
                    // dragging off the shelf never silently takes a file away.
                    let _ = set_hglobal(
                        &obj,
                        register_format("Preferred DropEffect"),
                        &DROPEFFECT_COPY.0.to_le_bytes(),
                    );
                    obj
                }
                Payload::Text(t) => text_data_object(t)?,
            };
            let allowed = match &payload {
                Payload::Files(_) => DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
                Payload::Text(_) => DROPEFFECT_COPY,
            };
            // A real drop source: SHDoDragDrop with no source gives the drag no way to
            // know when the button is released, so it fails or never ends.
            let source: IDropSource = DragSource.into();
            let effect = unsafe { SHDoDragDrop(Some(HWND(hwnd as *mut _)), &obj, Some(&source), allowed)? };
            Ok(effect.0)
        })();
        DRAG_OUT.store(false, Ordering::Relaxed);
        let _ = tx.send(res.map_err(|e| e.message().to_string()));
    })
    .map_err(err)?;
    let effect = rx.await.map_err(err)??;
    let effect = DROPEFFECT(effect);
    let name = if effect == DROPEFFECT_NONE {
        "none"
    } else if effect.0 & DROPEFFECT_MOVE.0 != 0 {
        "move"
    } else if effect.0 & DROPEFFECT_LINK.0 != 0 {
        "link"
    } else {
        "copy"
    };
    Ok(DragResult { effect: name.into() })
}

/// Put files on the clipboard as CF_HDROP so Ctrl+V pastes them anywhere.
pub fn copy_files_to_clipboard(paths: &[PathBuf]) -> CmdResult<()> {
    let bytes = dropfiles_bytes(paths);
    // Preferred DropEffect = copy, so Explorer pastes a copy rather than moving.
    let effect_cf = register_format("Preferred DropEffect");
    unsafe {
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            crate::util::sleep_ms(30);
        }
        if !opened {
            return Err("clipboard is busy".into());
        }
        let result = (|| -> windows::core::Result<()> {
            EmptyClipboard()?;
            let h = hglobal_from_bytes(&bytes)?;
            SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(h.0)))?;
            let h2 = hglobal_from_bytes(&DROPEFFECT_COPY.0.to_le_bytes())?;
            SetClipboardData(effect_cf as u32, Some(HANDLE(h2.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        crate::clipboard::ignore_seq(GetClipboardSequenceNumber());
        result.map_err(err)
    }
}

// ---------------------------------------------------------------------------
// Drag-in
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Incoming {
    File(PathBuf),
    /// Virtual file (e.g. an image dragged from a browser): name + bytes.
    Blob(String, Vec<u8>),
    Text(String),
}

#[implement(IDropTarget)]
struct DropTarget {
    app: AppHandle,
}

fn has_format(obj: &IDataObject, cf: u16) -> bool {
    let f = fmt(cf, (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32, -1);
    unsafe { obj.QueryGetData(&f).is_ok() }
}

fn acceptable(obj: &IDataObject) -> bool {
    has_format(obj, CF_HDROP.0)
        || has_format(obj, register_format("FileGroupDescriptorW"))
        || has_format(obj, CF_UNICODETEXT.0)
        || has_format(obj, register_format("UniformResourceLocatorW"))
}

fn medium_bytes(medium: &STGMEDIUM) -> Vec<u8> {
    unsafe {
        if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            hglobal_bytes(medium.u.hGlobal)
        } else if medium.tymed == TYMED_ISTREAM.0 as u32 {
            let mut out = Vec::new();
            if let Some(stream) = medium.u.pstm.as_ref() {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let mut read = 0u32;
                    let hr = stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read));
                    if hr.is_err() || read == 0 {
                        break;
                    }
                    out.extend_from_slice(&buf[..read as usize]);
                    if out.len() > 200 * 1024 * 1024 {
                        break;
                    }
                }
            }
            out
        } else {
            Vec::new()
        }
    }
}

fn get_bytes(obj: &IDataObject, cf: u16, lindex: i32) -> Option<Vec<u8>> {
    let f = fmt(cf, (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32, lindex);
    unsafe {
        let mut medium = obj.GetData(&f).ok()?;
        let bytes = medium_bytes(&medium);
        ReleaseStgMedium(&mut medium);
        Some(bytes)
    }
}

fn wide_bytes_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn read_incoming(obj: &IDataObject) -> Vec<Incoming> {
    let mut out = Vec::new();
    // 1. Real files.
    let f = fmt(CF_HDROP.0, TYMED_HGLOBAL.0 as u32, -1);
    unsafe {
        if let Ok(mut medium) = obj.GetData(&f) {
            let hdrop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                out.push(Incoming::File(PathBuf::from(String::from_utf16_lossy(&buf[..len]))));
            }
            ReleaseStgMedium(&mut medium);
        }
    }
    if !out.is_empty() {
        return out;
    }
    // 2. Virtual files (browser images, Outlook attachments).
    let fgd = register_format("FileGroupDescriptorW");
    let contents = register_format("FileContents");
    if let Some(desc) = get_bytes(obj, fgd, -1) {
        const DESC_SIZE: usize = 592;
        const NAME_OFFSET: usize = 72;
        if desc.len() >= 4 {
            let count = u32::from_le_bytes([desc[0], desc[1], desc[2], desc[3]]) as usize;
            for i in 0..count.min(64) {
                let start = 4 + i * DESC_SIZE;
                if desc.len() < start + DESC_SIZE {
                    break;
                }
                let name = wide_bytes_to_string(&desc[start + NAME_OFFSET..start + DESC_SIZE]);
                if let Some(bytes) = get_bytes(obj, contents, i as i32)
                    && !bytes.is_empty()
                {
                    out.push(Incoming::Blob(name, bytes));
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    // 3. Text, then URLs.
    if let Some(b) = get_bytes(obj, CF_UNICODETEXT.0, -1) {
        let t = wide_bytes_to_string(&b);
        if !t.trim().is_empty() {
            out.push(Incoming::Text(t));
            return out;
        }
    }
    if let Some(b) = get_bytes(obj, register_format("UniformResourceLocatorW"), -1) {
        let t = wide_bytes_to_string(&b);
        if !t.trim().is_empty() {
            out.push(Incoming::Text(t));
        }
    }
    out
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let ok = !drag_out_active() && pdataobj.as_ref().map(acceptable).unwrap_or(false);
        unsafe { *pdweffect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        crate::notify::emit(
            "shelf-drag",
            serde_json::json!({ "state": "enter", "accept": ok, "own": drag_out_active() }),
        );
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *pdweffect = if drag_out_active() {
                DROPEFFECT_NONE
            } else {
                DROPEFFECT_COPY
            };
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        crate::notify::emit("shelf-drag", serde_json::json!({ "state": "leave" }));
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        crate::notify::emit("shelf-drag", serde_json::json!({ "state": "leave" }));
        // Dropping our own drag back onto the notch cancels it; the item stays.
        if drag_out_active() {
            unsafe { *pdweffect = DROPEFFECT_NONE };
            return Ok(());
        }
        let items = pdataobj.as_ref().map(read_incoming).unwrap_or_default();
        unsafe {
            *pdweffect = if items.is_empty() {
                DROPEFFECT_NONE
            } else {
                DROPEFFECT_COPY
            }
        };
        if !items.is_empty() {
            let app = self.app.clone();
            std::thread::spawn(move || crate::shelf::add_incoming(&app, items));
        }
        Ok(())
    }
}

unsafe extern "system" fn collect_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
    list.push(hwnd.0 as isize);
    BOOL(1)
}

/// Replace WebView2's drop target on the notch window (and its children) with ours.
pub fn install_drop_target(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let Ok(hwnd) = win.hwnd() else { return };
    let root = hwnd.0 as isize;
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || unsafe {
        let _ = OleInitialize(None);
        let mut handles: Vec<isize> = vec![root];
        let _ = EnumChildWindows(
            Some(HWND(root as *mut _)),
            Some(collect_child),
            LPARAM(&mut handles as *mut Vec<isize> as isize),
        );
        let target: IDropTarget = DropTarget { app: app2 }.into();
        for h in handles {
            let hwnd = HWND(h as *mut _);
            let _ = RevokeDragDrop(hwnd);
            let _ = RegisterDragDrop(hwnd, &target);
        }
    });
}

pub fn path_exists(p: &Path) -> bool {
    std::fs::metadata(p).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdrop_paths(obj: &IDataObject) -> Vec<String> {
        read_incoming(obj)
            .into_iter()
            .filter_map(|i| match i {
                Incoming::File(p) => Some(p.to_string_lossy().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shell_data_object_lists_every_file() {
        unsafe {
            let _ = OleInitialize(None);
        }
        let dir = tempfile::tempdir().unwrap();
        let a_dir = dir.path().join("a");
        let b_dir = dir.path().join("b");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let a1 = a_dir.join("one.txt");
        let a2 = a_dir.join("two.txt");
        let b1 = b_dir.join("three.txt");
        for p in [&a1, &a2, &b1] {
            std::fs::write(p, "x").unwrap();
        }
        // Same folder.
        let obj = files_data_object(&[a1.clone(), a2.clone()]).unwrap();
        assert_eq!(hdrop_paths(&obj).len(), 2);
        // Different folders.
        let obj = files_data_object(&[a1.clone(), b1.clone()]).unwrap();
        let got = hdrop_paths(&obj);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got.iter().any(|p| p.ends_with("three.txt")));
        // Text.
        let obj = text_data_object("hello").unwrap();
        assert!(matches!(read_incoming(&obj).first(), Some(Incoming::Text(t)) if t == "hello"));
    }

    #[test]
    fn dropfiles_layout() {
        let bytes = dropfiles_bytes(&[PathBuf::from("C:\\a.txt"), PathBuf::from("C:\\b c.txt")]);
        let header = std::mem::size_of::<DROPFILES>();
        let names = wide_bytes_to_string(&bytes[header..]);
        assert_eq!(names, "C:\\a.txt");
        // Ends with a double null terminator.
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
    }
}
