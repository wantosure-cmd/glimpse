//! Recycle first. Permanent deletion is a separate explicit-confirmation operation.
use crate::domain::{Fingerprint, MediaId};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct TrashRequest {
    pub id: MediaId,
    pub directory: bool,
    pub empty_only: bool,
    pub root: PathBuf,
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
}
#[derive(Clone, Debug)]
pub struct TrashReceipt {
    pub id: MediaId,
    pub path: PathBuf,
    pub recycled_path: PathBuf,
    pub pre_delete_flags: u32,
}

fn validate(request: &TrashRequest) -> Result<(), String> {
    #[cfg(windows)]
    if crate::media_path::virtual_hint(&request.path)
        && std::fs::canonicalize(&request.path)
            .is_err_and(|e| matches!(e.raw_os_error(), Some(1 | 50 | 1005)))
    {
        // A missing final-path API does not prove the disk lacks recycling.
        // Validate the absolute virtual path, then let Shell attempt recycle.
        return validate_permanent(request);
    }
    let root = std::fs::canonicalize(&request.root).map_err(|e| e.to_string())?;
    let path = std::fs::canonicalize(&request.path).map_err(|e| e.to_string())?;
    if path != request.path || !path.starts_with(&root) || path == root {
        return Err(crate::ui_text!(msg_90d4ef342114).into());
    }
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if if request.directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    } {
        return Err(crate::ui_text!(msg_f6ec91622b21).into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(crate::ui_text!(msg_9ccdd7970528).into());
        }
    }
    if Fingerprint::read(&path)? != request.fingerprint {
        return Err(crate::ui_text!(msg_9a18ed96a023).into());
    }
    Ok(())
}
/// Runs on a worker. Refuse links/junctions anywhere in a selected directory.
pub fn validate_folder(request: &TrashRequest) -> Result<(), String> {
    if !request.directory {
        return Err("Not a folder request".into());
    }
    validate(request)?;
    validate_folder_contents(request)
}
fn validate_folder_contents(request: &TrashRequest) -> Result<(), String> {
    if request.empty_only
        && std::fs::read_dir(&request.path)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Folder is no longer empty; kept in place".into());
    }
    let mut pending = vec![request.path.clone()];
    let mut visited = 0usize;
    while let Some(path) = pending.pop() {
        for item in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let meta = std::fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
            visited += 1;
            if visited > 100_000 {
                return Err("Folder exceeds recycle validation limit (100000 entries)".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Folder contains a reparse point; recycle refused".into());
                }
            }
            if meta.file_type().is_symlink() {
                return Err("Folder contains a symbolic link".into());
            }
            if meta.is_dir() {
                pending.push(item.path());
            }
        }
    }
    Ok(())
}
/// Only known recycle capability failures qualify for a UI confirmation offer.
pub fn can_offer_permanent(error: &str) -> bool {
    for id in ["msg_6749d6897e13", "msg_491fc4e329fc"] {
        let (zh, en) = crate::ui_text::variants(id);
        if error == zh || error == en {
            return true;
        }
    }
    let (zh, en) = crate::ui_text::variants("msg_ff443d52ba30");
    error.starts_with(zh) || error.starts_with(en)
}

/// Read-only validation for a user-confirmed permanent request. Virtual volumes may
/// lack final-path lookup; absolute paths and every ancestor still reject reparse points.
fn validate_permanent(request: &TrashRequest) -> Result<(), String> {
    let root = crate::media_path::resolve_read(&request.root).map_err(|e| e.to_string())?;
    let path = crate::media_path::resolve_read(&request.path).map_err(|e| e.to_string())?;
    if path != request.path || !path.starts_with(&root) || path == root {
        return Err(crate::ui_text!(msg_90d4ef342114).into());
    }
    for ancestor in path.ancestors() {
        let meta = std::fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(crate::ui_text!(msg_943944c69e04).into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(crate::ui_text!(msg_5833456154d1).into());
            }
        }
    }
    let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if (request.directory && !meta.is_dir()) || (!request.directory && !meta.is_file()) {
        return Err(crate::ui_text!(msg_8ca4a972ae98).into());
    }
    if Fingerprint::read(&path)? != request.fingerprint {
        return Err(crate::ui_text!(msg_9a18ed96a023).into());
    }
    Ok(())
}
/// Prepare only; never mutates anything. Used before the ten-second undo countdown.
pub fn prepare_folder(request: &TrashRequest) -> Result<(), String> {
    validate_permanent(request)?;
    validate_folder_contents(request)
}
/// Distinguish refusal before mutation from a directory removal that may be partial.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermanentFailure {
    pub message: String,
    pub may_have_deleted_contents: bool,
}
impl PermanentFailure {
    fn not_started(message: String) -> Self {
        Self {
            message,
            may_have_deleted_contents: false,
        }
    }
}
impl std::fmt::Display for PermanentFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.may_have_deleted_contents {
            write!(f, "{}", crate::ui_format!(msg_57f9dea197b2, self.message))
        } else {
            self.message.fmt(f)
        }
    }
}
/// Must only be called from the explicit, per-batch confirmation path.
pub fn delete_permanently_confirmed(request: &TrashRequest) -> Result<(), PermanentFailure> {
    validate_permanent(request).map_err(PermanentFailure::not_started)?;
    if request.directory {
        validate_folder_contents(request).map_err(PermanentFailure::not_started)?;
    }
    validate_permanent(request).map_err(PermanentFailure::not_started)?;
    if request.directory && request.empty_only {
        std::fs::remove_dir(&request.path)
    } else if request.directory {
        std::fs::remove_dir_all(&request.path)
    } else {
        std::fs::remove_file(&request.path)
    }
    .map_err(|e| PermanentFailure {
        message: e.to_string(),
        may_have_deleted_contents: request.directory,
    })
}
pub fn recycle(request: &TrashRequest) -> Result<TrashReceipt, String> {
    if request.directory {
        validate_folder(request)?;
    } else {
        validate(request)?;
    }
    #[cfg(windows)]
    {
        windows_backend::recycle(request)
    }
    #[cfg(not(windows))]
    {
        Err(crate::ui_text!(msg_5ea8bf667ee1).into())
    }
}

#[derive(Debug)]
pub enum TrashEvent {
    PermanentItem {
        batch: u64,
        request: TrashRequest,
        result: Result<(), PermanentFailure>,
    },
    Item {
        batch: u64,
        request: TrashRequest,
        result: Result<TrashReceipt, String>,
    },
    Finished {
        batch: u64,
    },
}
enum Command {
    Batch(u64, Vec<TrashRequest>, bool),
    Stop,
}
pub struct TrashService {
    commands: std::sync::mpsc::SyncSender<Command>,
    events: std::sync::mpsc::Receiver<TrashEvent>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl TrashService {
    /// Caller must first prevent new decodes and acknowledge release of source handles.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (commands, rx) = mpsc::sync_channel(1);
        let (tx, events) = mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker = std::thread::spawn(move || {
            let send = |mut event| {
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        return false;
                    }
                    match tx.try_send(event) {
                        Ok(()) => {
                            wake();
                            return true;
                        }
                        Err(mpsc::TrySendError::Disconnected(_)) => return false,
                        Err(mpsc::TrySendError::Full(returned)) => {
                            event = returned;
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                    }
                }
            };
            while !cancelled.load(Ordering::Acquire) {
                let Ok(Command::Batch(batch, requests, permanent)) = rx.recv() else {
                    return;
                };
                for request in requests {
                    if cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    let event = if permanent {
                        let result = delete_permanently_confirmed(&request);
                        TrashEvent::PermanentItem {
                            batch,
                            request,
                            result,
                        }
                    } else {
                        let result = recycle(&request);
                        TrashEvent::Item {
                            batch,
                            request,
                            result,
                        }
                    };
                    if !send(event) {
                        return;
                    }
                }
                if !send(TrashEvent::Finished { batch }) {
                    return;
                }
            }
        });
        Self {
            commands,
            events,
            stop,
            worker: Some(worker),
        }
    }
    pub fn submit(&self, batch: u64, requests: Vec<TrashRequest>) -> Result<(), String> {
        self.submit_mode(batch, requests, false)
    }
    pub fn submit_permanent_confirmed(
        &self,
        batch: u64,
        requests: Vec<TrashRequest>,
    ) -> Result<(), String> {
        self.submit_mode(batch, requests, true)
    }
    fn submit_mode(
        &self,
        batch: u64,
        requests: Vec<TrashRequest>,
        permanent: bool,
    ) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        if requests.is_empty()
            || requests
                .iter()
                .any(|r| !ids.insert(r.id) || !paths.insert(r.path.clone()))
        {
            return Err(crate::ui_text!(msg_d0d339f95699).into());
        }
        self.commands
            .try_send(Command::Batch(batch, requests, permanent))
            .map_err(|_| crate::ui_text!(msg_d45bac635acc).into())
    }
    pub fn poll(&self) -> Option<TrashEvent> {
        self.events.try_recv().ok()
    }
}
impl Drop for TrashService {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = self.commands.try_send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use std::{
        cell::RefCell,
        os::windows::ffi::{OsStrExt, OsStringExt},
        rc::Rc,
    };
    use windows::{
        Win32::{System::Com::*, UI::Shell::*},
        core::{Error, HRESULT, PCWSTR, Ref, implement},
    };
    struct Apartment;
    impl Apartment {
        fn new() -> windows::core::Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            }
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    fn aborted(message: impl AsRef<str>) -> Error {
        Error::new(HRESULT(0x80004004u32 as i32), message.as_ref())
    }
    fn explain(error: Error) -> String {
        let code = error.code();
        let reason = match code {
            COPYENGINE_E_SHARING_VIOLATION_SRC | COPYENGINE_E_SHARING_VIOLATION_DEST => {
                crate::ui_text!(msg_96529cbfc322)
            }
            COPYENGINE_E_ACCESS_DENIED_SRC
            | COPYENGINE_E_ACCESS_DENIED_DEST
            | COPYENGINE_E_REQUIRES_ELEVATION => crate::ui_text!(msg_e8b482993477),
            COPYENGINE_E_RECYCLE_FORCE_NUKE
            | COPYENGINE_E_RECYCLE_SIZE_TOO_BIG
            | COPYENGINE_E_RECYCLE_BIN_NOT_FOUND
            | COPYENGINE_E_RECYCLE_PATH_TOO_LONG => {
                crate::ui_text!(msg_ff443d52ba30)
            }
            _ => {
                return crate::ui_format!(msg_07eeba75fdb2, code.0 as u32, error = error);
            }
        };
        format!("{reason}（0x{:08X}）", code.0 as u32)
    }
    fn wide(path: &std::path::Path) -> Vec<u16> {
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
        if units.starts_with(&[92, 92, 63, 92]) {
            units.drain(..4);
        }
        units.push(0);
        units
    }
    fn item(path: &std::path::Path) -> windows::core::Result<IShellItem> {
        let name = wide(path);
        unsafe { SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None) }
    }
    fn item_path(item: &IShellItem) -> windows::core::Result<PathBuf> {
        unsafe {
            let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let owned = std::ffi::OsString::from_wide(path.as_wide());
            CoTaskMemFree(Some(path.0.cast()));
            Ok(PathBuf::from(owned))
        }
    }
    fn same_shell_path(a: &std::path::Path, b: &std::path::Path) -> bool {
        use std::os::windows::ffi::OsStrExt;
        fn normalized(p: &std::path::Path) -> Vec<u16> {
            let mut w: Vec<_> = p.as_os_str().encode_wide().collect();
            if w.starts_with(&[92, 92, 63, 92, 85, 78, 67, 92]) {
                w.splice(..8, [92, 92]);
            } else if w.starts_with(&[92, 92, 63, 92]) {
                w.drain(..4);
            }
            w
        }
        normalized(a) == normalized(b)
    }
    #[derive(Default)]
    struct Report {
        checked: bool,
        pre_flags: u32,
        posted: Option<Result<PathBuf, String>>,
        rejected: Option<String>,
    }
    #[implement(IFileOperationProgressSink)]
    struct Sink {
        request: TrashRequest,
        report: Rc<RefCell<Report>>,
    }
    #[allow(non_snake_case, unused_variables)]
    impl IFileOperationProgressSink_Impl for Sink_Impl {
        fn PreDeleteItem(&self, flags: u32, shell: Ref<IShellItem>) -> windows::core::Result<()> {
            let result = (|| -> Result<(), String> {
                if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
                    return Err(crate::ui_text!(msg_6749d6897e13).into());
                }
                let actual =
                    item_path(shell.ok().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let actual = crate::media_path::resolve_read(&actual).map_err(|e| e.to_string())?;
                if self.request.empty_only {
                    if actual != self.request.path {
                        return Err("Empty-only recycle cannot include child items".into());
                    }
                    validate_folder_contents(&self.request)?;
                }
                if self.request.directory && actual != self.request.path {
                    if !actual.starts_with(&self.request.path) {
                        return Err("Folder recycle escaped its source".into());
                    }
                    let meta = std::fs::symlink_metadata(&actual).map_err(|e| e.to_string())?;
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err("Reparse child refused".into());
                    }
                    return Ok(());
                }
                if actual != self.request.path {
                    return Err(crate::ui_text!(msg_aed5785a3fb7).into());
                }
                validate(&self.request)
            })();
            match result {
                Ok(()) => {
                    self.report.borrow_mut().checked = true;
                    self.report.borrow_mut().pre_flags = flags;
                    Ok(())
                }
                Err(message) => {
                    self.report.borrow_mut().rejected = Some(message.clone());
                    Err(aborted(message))
                }
            }
        }
        fn PostDeleteItem(
            &self,
            flags: u32,
            shell: Ref<IShellItem>,
            result: HRESULT,
            new: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            if self.request.directory {
                if let Some(shell) = shell.as_ref() {
                    if item_path(shell)
                        .ok()
                        .is_none_or(|p| !same_shell_path(&p, &self.request.path))
                    {
                        return result.ok();
                    }
                }
            }
            let posted = (|| -> Result<PathBuf, String> {
                result.ok().map_err(explain)?;
                if !self.report.borrow().checked {
                    return Err(crate::ui_text!(msg_e558a4889ba7).into());
                }
                let new = new.as_ref().ok_or(crate::ui_text!(msg_1082fd85340c))?;
                item_path(new).map_err(|e| e.to_string())
            })();
            self.report.borrow_mut().posted = Some(posted);
            Ok(())
        }
        fn StartOperations(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, hrresult: windows::core::HRESULT) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreRenameItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Err(aborted("Unexpected operation rejected"))
        }
        fn PostRenameItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
            hrrename: windows::core::HRESULT,
            psinewlycreated: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Err(aborted("Unexpected operation rejected"))
        }
        fn PostMoveItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
            hrmove: windows::core::HRESULT,
            psinewlycreated: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Err(aborted("Unexpected operation rejected"))
        }
        fn PostCopyItem(
            &self,
            dwflags: u32,
            psiitem: windows::core::Ref<IShellItem>,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
            hrcopy: windows::core::HRESULT,
            psinewlycreated: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreNewItem(
            &self,
            dwflags: u32,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Err(aborted("Unexpected operation rejected"))
        }
        fn PostNewItem(
            &self,
            dwflags: u32,
            psidestinationfolder: windows::core::Ref<IShellItem>,
            psznewname: &windows::core::PCWSTR,
            psztemplatename: &windows::core::PCWSTR,
            dwfileattributes: u32,
            hrnew: windows::core::HRESULT,
            psinewitem: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn UpdateProgress(&self, iworktotal: u32, iworksofar: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
    }
    pub(super) fn recycle(request: &TrashRequest) -> Result<TrashReceipt, String> {
        // Network shares do not offer the local Windows recycle-bin contract.
        if request
            .path
            .as_os_str()
            .encode_wide()
            .take(8)
            .eq(r"\\?\UNC\".encode_utf16())
        {
            return Err(crate::ui_text!(msg_491fc4e329fc).into());
        }
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let report = Rc::new(RefCell::new(Report::default()));
        let sink: IFileOperationProgressSink = Sink {
            request: request.clone(),
            report: report.clone(),
        }
        .into();
        let operation: IFileOperation =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| e.to_string())?;
        let source = item(&request.path).map_err(|e| e.to_string())?;
        let performed = unsafe {
            operation
                .SetOperationFlags(
                    FOFX_RECYCLEONDELETE
                        | FOFX_ADDUNDORECORD
                        | FOFX_EARLYFAILURE
                        | FOF_NOERRORUI
                        | FOF_SILENT
                        | FOF_NOCONFIRMATION
                        | FOF_NO_CONNECTED_ELEMENTS,
                )
                .and_then(|_| operation.DeleteItem(&source, &sink))
                .and_then(|_| operation.PerformOperations())
        };
        let mut state = report.borrow_mut();
        if let Some(message) = state.rejected.take() {
            return Err(message);
        }
        performed.map_err(explain)?;
        if unsafe { operation.GetAnyOperationsAborted() }
            .map_err(|e| e.to_string())?
            .as_bool()
        {
            return Err(crate::ui_text!(msg_e7dc95985655).into());
        }
        let recycled_path = state
            .posted
            .take()
            .ok_or(crate::ui_text!(msg_0df8b18d273a))??;
        Ok(TrashReceipt {
            id: request.id,
            path: request.path.clone(),
            recycled_path,
            pre_delete_flags: state.pre_flags,
        })
    }

    #[cfg(test)]
    pub(super) fn check_callback_guards(request: &TrashRequest) {
        let _apartment = Apartment::new().unwrap();
        let source = item(&request.path).unwrap();
        let report = Rc::new(RefCell::new(Report::default()));
        let sink: IFileOperationProgressSink = Sink {
            request: request.clone(),
            report: report.clone(),
        }
        .into();
        // Invoke our real COM vtable. No file operation is scheduled by this test.
        assert!(unsafe { sink.PreDeleteItem(0, &source) }.is_err());
        assert!(!report.borrow().checked);
        assert!(
            report
                .borrow()
                .rejected
                .as_ref()
                .unwrap()
                .contains("永久删除")
        );
        let mut stale = request.clone();
        stale.fingerprint.bytes += 1;
        let report = Rc::new(RefCell::new(Report::default()));
        let sink: IFileOperationProgressSink = Sink {
            request: stale,
            report: report.clone(),
        }
        .into();
        assert!(
            unsafe { sink.PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, &source) }
                .is_err()
        );
        assert!(!report.borrow().checked);
        assert!(report.borrow().rejected.as_ref().unwrap().contains("刷新"));
    }

    #[cfg(test)]
    pub(super) fn bin_contains(receipt: &TrashReceipt) -> Result<bool, String> {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let bin: IShellItem =
            unsafe { SHGetKnownFolderItem(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT, None) }
                .map_err(|e| e.to_string())?;
        let entries: IEnumShellItems =
            unsafe { bin.BindToHandler(None, &BHID_EnumItems) }.map_err(|e| e.to_string())?;
        loop {
            let mut next = [None];
            let mut fetched = 0;
            unsafe { entries.Next(&mut next, Some(&mut fetched)) }.map_err(|e| e.to_string())?;
            if fetched == 0 {
                return Ok(false);
            }
            let entry = next[0].take().ok_or("Missing enumerated shell item")?;
            let actual = item_path(&entry).map_err(|e| e.to_string())?;
            if actual
                .to_string_lossy()
                .eq_ignore_ascii_case(&receipt.recycled_path.to_string_lossy())
            {
                return Ok(true);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn restore_copy(receipt: &TrashReceipt) -> Result<(), String> {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        if receipt.path.exists() {
            return Err("恢复目标已存在，拒绝覆盖".into());
        }
        // Use the bin's Restore verb, not a filesystem move of its $R storage file.
        let bin: IShellItem =
            unsafe { SHGetKnownFolderItem(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT, None) }
                .map_err(|e| e.to_string())?;
        let entries: IEnumShellItems =
            unsafe { bin.BindToHandler(None, &BHID_EnumItems) }.map_err(|e| e.to_string())?;
        let expected = std::fs::canonicalize(&receipt.recycled_path).map_err(|e| e.to_string())?;
        let mut source = None;
        loop {
            let mut next = [None];
            let mut fetched = 0;
            unsafe { entries.Next(&mut next, Some(&mut fetched)) }.map_err(|e| e.to_string())?;
            if fetched == 0 {
                break;
            }
            if let Some(entry) = next[0].take() {
                if item_path(&entry)
                    .ok()
                    .and_then(|p| std::fs::canonicalize(p).ok())
                    .as_ref()
                    == Some(&expected)
                {
                    source = Some(entry);
                    break;
                }
            }
        }
        let source = source.ok_or("专属测试副本不在回收站命名空间中")?;
        let context: IContextMenu =
            unsafe { source.BindToHandler(None, &BHID_SFUIObject) }.map_err(|e| e.to_string())?;
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu};
            let menu = CreatePopupMenu().map_err(|e| e.to_string())?;
            let initialized = context.QueryContextMenu(menu, 0, 1, 0x7fff, CMF_NORMAL);
            let _ = DestroyMenu(menu);
            initialized.ok().map_err(|e| e.to_string())?;
            let command = CMINVOKECOMMANDINFO {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                fMask: SEE_MASK_FLAG_NO_UI,
                lpVerb: windows::core::PCSTR(c"undelete".as_ptr().cast()),
                ..Default::default()
            };
            context.InvokeCommand(&command).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    fn native_recycle_and_restore_dedicated_copy() {
        fn exercise_native_copy() {
            let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
            std::fs::create_dir_all(workspace.join("artifacts")).unwrap();
            std::fs::write(
                workspace.join("artifacts/p4-trash-native.json"),
                b"{\"passed\":false,\"state\":\"running\"}",
            )
            .unwrap();
            let base = workspace.join("target/test-fixtures").join(format!(
                "recycle-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&base).unwrap();
            let original = base.join("original.png");
            image::RgbImage::from_pixel(12, 9, image::Rgb([30, 100, 200]))
                .save(&original)
                .unwrap();
            let bytes = std::fs::read(&original).unwrap();
            let copy = base.join("专属回收副本.png");
            std::fs::copy(&original, &copy).unwrap();
            let copy = std::fs::canonicalize(copy).unwrap();
            assert!(copy.starts_with(&base));
            let request = TrashRequest {
                empty_only: false,
                directory: false,
                id: 1,
                root: base.clone(),
                path: copy.clone(),
                fingerprint: Fingerprint::read(&copy).unwrap(),
            };
            windows_backend::check_callback_guards(&request);
            assert_eq!(std::fs::read(&copy).unwrap(), bytes);
            let receipt = recycle(&request).expect("Dedicated copy must really be recycled");
            let info = receipt.recycled_path.with_file_name(
                receipt
                    .recycled_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replacen("$R", "$I", 1),
            );
            assert!(info.exists(), "Recycle metadata must exist before restore");
            assert!(
                windows_backend::bin_contains(&receipt).unwrap(),
                "Dedicated copy must be listed in the system recycle namespace"
            );
            assert!(!copy.exists());
            assert!(
                receipt.recycled_path.exists(),
                "Returned recycle item must exist"
            );
            assert_eq!(std::fs::read(&receipt.recycled_path).unwrap(), bytes);
            windows_backend::restore_copy(&receipt).expect("Our recycled copy must be restorable");
            assert!(
                !windows_backend::bin_contains(&receipt).unwrap(),
                "Restored copy must disappear from the system recycle namespace"
            );
            assert_eq!(std::fs::read(&copy).unwrap(), bytes);
            assert_eq!(std::fs::read(&original).unwrap(), bytes);
            let mut stale = request.clone();
            stale.fingerprint.bytes += 1;
            assert!(recycle(&stale).is_err());
            assert_eq!(std::fs::read(&copy).unwrap(), bytes);
            let stale_copy = base.join("保留的过期副本.png");
            std::fs::copy(&original, &stale_copy).unwrap();
            let stale_copy = std::fs::canonicalize(stale_copy).unwrap();
            let mut bad = TrashRequest {
                empty_only: false,
                directory: false,
                id: 2,
                root: base.clone(),
                path: stale_copy.clone(),
                fingerprint: Fingerprint::read(&stale_copy).unwrap(),
            };
            bad.fingerprint.bytes += 1;
            let locked_copy = base.join("占用副本.png");
            std::fs::copy(&original, &locked_copy).unwrap();
            let locked_copy = std::fs::canonicalize(locked_copy).unwrap();
            let locked = TrashRequest {
                empty_only: false,
                directory: false,
                id: 3,
                root: base.clone(),
                path: locked_copy.clone(),
                fingerprint: Fingerprint::read(&locked_copy).unwrap(),
            };
            use std::os::windows::fs::OpenOptionsExt;
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked_copy)
                .unwrap();
            let current = TrashRequest {
                empty_only: false,
                fingerprint: Fingerprint::read(&copy).unwrap(),
                ..request.clone()
            };
            let service = TrashService::new(|| {});
            assert!(
                service
                    .submit(99, vec![current.clone(), current.clone()])
                    .is_err()
            );
            service.submit(7, vec![bad, locked, current]).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut successes = 0;
            let mut failures = 0;
            let mut failure_messages = std::collections::BTreeMap::new();
            let mut recovered = None;
            loop {
                assert!(
                    std::time::Instant::now() < deadline,
                    "Recycle worker timeout"
                );
                match service.poll() {
                    Some(TrashEvent::Item {
                        batch,
                        request,
                        result,
                    }) => {
                        assert_eq!(batch, 7);
                        match result {
                            Ok(receipt) => {
                                assert_eq!(request.id, 1);
                                successes += 1;
                                recovered = Some(receipt);
                            }
                            Err(message) => {
                                assert!([2, 3].contains(&request.id));
                                assert!(failure_messages.insert(request.id, message).is_none());
                                failures += 1;
                            }
                        }
                    }
                    Some(TrashEvent::PermanentItem { .. }) => {
                        panic!("Recycle batch must never switch to permanent deletion")
                    }
                    Some(TrashEvent::Finished { batch }) => {
                        assert_eq!(batch, 7);
                        break;
                    }
                    None => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
            assert_eq!((successes, failures), (1, 2));
            assert_eq!(
                failure_messages.keys().copied().collect::<Vec<_>>(),
                vec![2, 3]
            );
            assert!(failure_messages[&3].contains("占用"));
            drop(lock);
            assert_eq!(std::fs::read(&locked_copy).unwrap(), bytes);
            assert_eq!(std::fs::read(&stale_copy).unwrap(), bytes);
            let recovered = recovered.unwrap();
            let info2 = recovered.recycled_path.with_file_name(
                recovered
                    .recycled_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replacen("$R", "$I", 1),
            );
            windows_backend::restore_copy(&recovered).unwrap();
            assert!(
                !windows_backend::bin_contains(&recovered).unwrap(),
                "Worker copy remained in recycle namespace"
            );
            assert_eq!(std::fs::read(&copy).unwrap(), bytes);
            assert_eq!(std::fs::read(&original).unwrap(), bytes);
            let evidence = serde_json::json!({"passed":true,"fixture":base,"copy":copy,"recycle_path":receipt.recycled_path,"pre_delete_flags":receipt.pre_delete_flags,"restored":true,"system_recycle_entry_removed_after_restore":true,"physical_metadata_retained_after_restore":info.exists(),"worker_physical_metadata_retained_after_restore":info2.exists(),"original_unchanged":true,"stale_fingerprint_rejected":true,"callback_non_recycle_rejected":true,"callback_stale_fingerprint_rejected":true,"worker_partial_failure":{"succeeded":successes,"failed":failures,"failure_messages":failure_messages,"failed_copy_unchanged":true,"locked_copy_unchanged":true,"successful_copy_restored":true,"duplicate_batch_rejected":true},"scope":"Actual Windows IFileOperation recycle, namespace listing and native Restore verb for generated dedicated copies; COM callback guards and bounded worker stale/locked partial results; physical $I record retention reported, not claimed cleared; no product UI or permanent-delete fallback"});
            std::fs::write(
                workspace.join("artifacts/p4-trash-native.json"),
                serde_json::to_vec_pretty(&evidence).unwrap(),
            )
            .unwrap();
        }
        std::thread::spawn(exercise_native_copy).join().unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn native_folder_recycle_preserves_nested_non_media_and_restores() {
        std::thread::spawn(|| {
            let base = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap().join("target/test-fixtures").join(format!("folder-recycle-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            let folder = base.join("专用目录");
            std::fs::create_dir_all(folder.join("子目录")).unwrap();
            std::fs::write(folder.join("子目录/notes.txt"), b"non-media must survive recycling").unwrap();
            std::fs::write(folder.join("photo.jpg"), b"dedicated generated fixture").unwrap();
            let request = TrashRequest { empty_only: false, directory: true, id: -1, root: base.clone(), path: folder.clone(), fingerprint: Fingerprint::read(&folder).unwrap() };
            validate_folder(&request).unwrap();
            let mut bad = request.clone(); bad.root = folder.clone();
            assert!(validate_folder(&bad).is_err());
            bad = request.clone(); bad.fingerprint.modified_ns += 1;
            assert!(validate_folder(&bad).is_err());
            let receipt = recycle(&request).expect("Actual folder recycle");
            assert!(!folder.exists());
            assert!(windows_backend::bin_contains(&receipt).unwrap());
            assert_eq!(std::fs::read(receipt.recycled_path.join("子目录/notes.txt")).unwrap(), b"non-media must survive recycling");
            windows_backend::restore_copy(&receipt).unwrap();
            assert_eq!(std::fs::read(folder.join("子目录/notes.txt")).unwrap(), b"non-media must survive recycling");
            assert_eq!(std::fs::read(folder.join("photo.jpg")).unwrap(), b"dedicated generated fixture");
            let evidence = serde_json::json!({"checks_pass":true,"scope":"generated dedicated directory only; actual Windows recycle and restore", "fixture":base,"nested_non_media_preserved":true,"root_rejected":true,"stale_fingerprint_rejected":true,"recycled_path":receipt.recycled_path});
            std::fs::write(base.parent().unwrap().parent().unwrap().parent().unwrap().join("artifacts/folder-native-recycle.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        }).join().unwrap();
    }
    #[test]
    fn permanent_offer_excludes_permissions_occupancy_and_arbitrary_errors() {
        assert!(can_offer_permanent(crate::ui_text!(msg_6749d6897e13)));
        assert!(can_offer_permanent(crate::ui_text!(msg_491fc4e329fc)));
        assert!(!can_offer_permanent(crate::ui_text!(msg_e8b482993477)));
        assert!(!can_offer_permanent(crate::ui_text!(msg_96529cbfc322)));
        assert!(!can_offer_permanent("Access denied"));
    }
    #[test]
    fn confirmed_permanent_path_rejects_stale_and_root_then_deletes_only_generated_copy() {
        let base = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
            .unwrap()
            .join("target/test-fixtures")
            .join(format!(
                "permanent-confirmed-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let path = base.join("dedicated-copy");
        std::fs::create_dir_all(path.join("nested")).unwrap();
        std::fs::write(path.join("nested/non-media.txt"), b"generated copy").unwrap();
        let request = TrashRequest {
            empty_only: false,
            id: -1,
            directory: true,
            root: base.clone(),
            path: path.clone(),
            fingerprint: Fingerprint::read(&path).unwrap(),
        };
        let mut bad = request.clone();
        bad.root = path.clone();
        assert!(delete_permanently_confirmed(&bad).is_err());
        bad = request.clone();
        bad.fingerprint.modified_ns += 1;
        assert!(delete_permanently_confirmed(&bad).is_err());
        assert!(path.join("nested/non-media.txt").exists());
        delete_permanently_confirmed(&request).unwrap();
        assert!(!path.exists());
        assert!(base.exists());
    }
    #[test]
    #[cfg(windows)]
    fn permanent_locked_folder_reports_partial_and_batch_continues() {
        use std::os::windows::fs::OpenOptionsExt;
        let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let base = workspace.join("target/test-fixtures").join(format!(
            "permanent-partial-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let folder = base.join("partial");
        std::fs::create_dir_all(&folder).unwrap();
        for i in 0..10 {
            std::fs::write(
                folder.join(format!("{i:02}.txt")),
                format!("generated copy {i}"),
            )
            .unwrap();
        }
        let entries: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let locked = entries.last().unwrap().clone();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        let complete = base.join("complete");
        std::fs::create_dir(&complete).unwrap();
        std::fs::write(complete.join("copy.txt"), b"generated successful copy").unwrap();
        assert!(
            std::fs::canonicalize(&folder)
                .unwrap()
                .starts_with(workspace.join("target/test-fixtures"))
        );
        let request = TrashRequest {
            empty_only: false,
            id: -1,
            directory: true,
            root: base.clone(),
            path: folder.clone(),
            fingerprint: Fingerprint::read(&folder).unwrap(),
        };
        let mut stale = request.clone();
        stale.fingerprint.modified_ns += 1;
        let early = delete_permanently_confirmed(&stale).unwrap_err();
        assert!(!early.may_have_deleted_contents);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 10);
        let second = TrashRequest {
            empty_only: false,
            id: -2,
            directory: true,
            root: base.clone(),
            path: complete.clone(),
            fingerprint: Fingerprint::read(&complete).unwrap(),
        };
        let service = TrashService::new(|| {});
        service
            .submit_permanent_confirmed(931, vec![request.clone(), second])
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut failure = None;
        let mut successes = 0;
        loop {
            assert!(std::time::Instant::now() < deadline, "worker timeout");
            match service.poll() {
                Some(TrashEvent::PermanentItem {
                    batch,
                    request,
                    result,
                }) => {
                    assert_eq!(batch, 931);
                    if request.id == -1 {
                        failure = Some(result.unwrap_err());
                    } else {
                        result.unwrap();
                        successes += 1;
                    }
                }
                Some(TrashEvent::Finished { batch }) => {
                    assert_eq!(batch, 931);
                    break;
                }
                Some(TrashEvent::Item { .. }) => panic!("Confirmed batch changed mode"),
                None => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
        let failure = failure.unwrap();
        assert!(failure.may_have_deleted_contents, "{failure:?}");
        let remaining = std::fs::read_dir(&folder).unwrap().count();
        assert!(
            remaining < 10,
            "Expected an actual partially completed OS removal, not just a preflight failure"
        );
        assert!(locked.exists());
        assert!(!complete.exists());
        assert_eq!(successes, 1);
        drop(lock);
        let survivor = std::fs::read(&locked).unwrap();
        assert!(String::from_utf8_lossy(&survivor).starts_with("generated copy"));
        // Only a new explicit request after the lock is removed completes the remainder.
        let retry = TrashRequest {
            empty_only: false,
            fingerprint: Fingerprint::read(&folder).unwrap(),
            ..request
        };
        delete_permanently_confirmed(&retry).unwrap();
        assert!(!folder.exists());
        let report = serde_json::json!({"checks_pass":true,"scope":"actual Windows locked-file failure; generated dedicated copies only","fixture":base,"initial_files":10,"remaining_after_failure":remaining,"deleted_before_failure":10-remaining,"successful_targets":successes,"failed_targets":1,"failure":failure,"preflight_failure_mutated":false,"locked_survivor_unchanged":true,"explicit_retry_completed":true});
        std::fs::write(
            workspace.join("artifacts/permanent-partial-native.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn empty_only_never_accepts_hidden_or_nonmedia_contents() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("empty-only-test-{}", std::process::id()));
        std::fs::create_dir_all(base.join("empty")).unwrap();
        let root = std::fs::canonicalize(&base).unwrap();
        let path = root.join("empty");
        let mut request = TrashRequest {
            empty_only: true,
            directory: true,
            id: -1,
            root: root.clone(),
            path: path.clone(),
            fingerprint: Fingerprint::read(&path).unwrap(),
        };
        assert!(prepare_folder(&request).is_ok());
        std::fs::write(path.join(".keep.txt"), b"keep").unwrap();
        request.fingerprint = Fingerprint::read(&path).unwrap();
        assert!(prepare_folder(&request).is_err());
        assert!(delete_permanently_confirmed(&request).is_err());
        assert_eq!(std::fs::read(path.join(".keep.txt")).unwrap(), b"keep");
        std::fs::remove_file(path.join(".keep.txt")).unwrap();
        std::fs::remove_dir(path).unwrap();
        std::fs::remove_dir(base).unwrap();
    }
}
