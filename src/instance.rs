//! Same-user local IPC. Fixed protocol, bounded queue/buffer, cancellable overlapped I/O.
use crate::domain::{path_blob, path_from_blob};
use std::{
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{IO::*, Pipes::*, Threading::*},
        UI::WindowsAndMessaging::AllowSetForegroundWindow,
    },
    core::{HRESULT, PCWSTR, PWSTR},
};
const MAX_FRAME: usize = 65536;
const MAGIC: &[u8; 8] = b"MBIPC001";
type Wake = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub directory: Option<PathBuf>,
}
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn encode(request: &Request) -> Result<Vec<u8>, String> {
    let body = request
        .directory
        .as_ref()
        .map(|p| path_blob(p))
        .unwrap_or_default();
    if body.len() + 12 > MAX_FRAME {
        return Err(crate::ui_text!(msg_fee9acef3c93).into());
    }
    let mut frame = MAGIC.to_vec();
    frame.extend((body.len() as u32).to_le_bytes());
    frame.extend(body);
    Ok(frame)
}
fn decode(frame: &[u8]) -> Result<Request, String> {
    if frame.len() < 12 || frame.len() > MAX_FRAME || &frame[..8] != MAGIC {
        return Err(crate::ui_text!(msg_43376f5a02a5).into());
    }
    let length = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;
    if length != frame.len() - 12 || length % 2 != 0 {
        return Err(crate::ui_text!(msg_de6f7d2a0f88).into());
    }
    if length == 0 {
        return Ok(Request { directory: None });
    }
    if frame[12..].chunks_exact(2).any(|b| b == [0, 0]) {
        return Err(crate::ui_text!(msg_87461f335055).into());
    }
    Ok(Request {
        directory: Some(path_from_blob(&frame[12..])?),
    })
}
fn normalize(path: &Path) -> Result<PathBuf, String> {
    let path = crate::media_path::resolve_read(path)
        .map_err(|e| crate::ui_format!(msg_a1da108b966c, e = e))?;
    if !path.is_dir() {
        return Err(crate::ui_text!(msg_ee22fbc1a9f6).into());
    }
    Ok(path)
}
struct Handle(OwnedHandle);
impl Handle {
    unsafe fn new(h: HANDLE) -> Self {
        Self(unsafe { OwnedHandle::from_raw_handle(h.0) })
    }
    fn raw(&self) -> HANDLE {
        HANDLE(self.0.as_raw_handle())
    }
    fn event() -> Result<Self, String> {
        unsafe {
            CreateEventW(None, true, false, None)
                .map(|h| Self::new(h))
                .map_err(|e| e.to_string())
        }
    }
}
struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}
fn identity() -> Result<(String, Descriptor), String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| e.to_string())?;
        let token = Handle::new(token);
        let mut size = 0;
        let _ = GetTokenInformation(token.raw(), TokenUser, None, 0, &mut size);
        if size == 0 || size > 65536 {
            return Err(crate::ui_text!(msg_a7ef7d2aa9db).into());
        }
        let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        GetTokenInformation(
            token.raw(),
            TokenUser,
            Some(data.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .map_err(|e| e.to_string())?;
        let user = &*(data.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(|e| e.to_string())?;
        let sid = text.to_string().map_err(|e| e.to_string());
        LocalFree(Some(HLOCAL(text.0.cast())));
        let sid = sid?;
        let sddl = wide(std::ffi::OsStr::new(&format!("D:P(A;;GA;;;{sid})")));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            1,
            &mut descriptor,
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok((sid, Descriptor(descriptor)))
    }
}
fn finish_io(
    handle: HANDLE,
    stop: HANDLE,
    event: HANDLE,
    ov: &mut OVERLAPPED,
    initial: windows::core::Result<()>,
    count: u32,
    timeout: u32,
) -> Result<u32, String> {
    match initial {
        Ok(()) => return Ok(count),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_IO_PENDING.0) => {}
        Err(error) => return Err(error.to_string()),
    }
    unsafe {
        let wait = WaitForMultipleObjects(&[stop, event], false, timeout);
        if wait != WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
            let _ = CancelIoEx(handle, Some(ov));
            let mut ignored = 0;
            let _ = GetOverlappedResult(handle, ov, &mut ignored, true);
            return Err(if wait == WAIT_OBJECT_0 {
                crate::ui_text!(msg_477c0dedf9b3)
            } else {
                crate::ui_text!(msg_e06c6e3bb1bb)
            }
            .into());
        }
        let mut transferred = 0;
        GetOverlappedResult(handle, ov, &mut transferred, false).map_err(|e| e.to_string())?;
        Ok(transferred)
    }
}
fn read(handle: HANDLE, stop: HANDLE, buffer: &mut [u8], timeout: u32) -> Result<usize, String> {
    let event = Handle::event()?;
    let mut ov = OVERLAPPED {
        hEvent: event.raw(),
        ..Default::default()
    };
    let mut n = 0;
    let result = unsafe { ReadFile(handle, Some(buffer), Some(&mut n), Some(&mut ov)) };
    finish_io(handle, stop, event.raw(), &mut ov, result, n, timeout).map(|n| n as usize)
}
fn write(handle: HANDLE, stop: HANDLE, buffer: &[u8], timeout: u32) -> Result<(), String> {
    let event = Handle::event()?;
    let mut ov = OVERLAPPED {
        hEvent: event.raw(),
        ..Default::default()
    };
    let mut n = 0;
    let result = unsafe { WriteFile(handle, Some(buffer), Some(&mut n), Some(&mut ov)) };
    let n = finish_io(handle, stop, event.raw(), &mut ov, result, n, timeout)?;
    if n as usize != buffer.len() {
        return Err(crate::ui_text!(msg_0e1cf0e8af37).into());
    }
    Ok(())
}
fn connect(handle: HANDLE, stop: HANDLE) -> Result<(), String> {
    let event = Handle::event()?;
    let mut ov = OVERLAPPED {
        hEvent: event.raw(),
        ..Default::default()
    };
    let result = unsafe { ConnectNamedPipe(handle, Some(&mut ov)) };
    if result
        .as_ref()
        .err()
        .is_some_and(|e| e.code() == HRESULT::from_win32(ERROR_PIPE_CONNECTED.0))
    {
        return Ok(());
    }
    finish_io(handle, stop, event.raw(), &mut ov, result, 0, INFINITE).map(|_| ())
}
pub enum Entry {
    Primary(Primary),
    Forwarded { server_pid: u32 },
}
pub struct Primary {
    _mutex: Handle,
    stop: Arc<Handle>,
    worker: Option<thread::JoinHandle<()>>,
    receiver: mpsc::Receiver<Request>,
    wake: Wake,
    pub pipe_name: String,
    #[cfg(test)]
    pipe_dacl: String,
}
impl Primary {
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self.wake.lock().unwrap() = Some(Arc::new(wake));
    }
    pub fn poll(&self) -> Option<Request> {
        self.receiver.try_recv().ok()
    }
}
impl Drop for Primary {
    fn drop(&mut self) {
        unsafe {
            let _ = SetEvent(self.stop.raw());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub fn enter(scope: &str, directory: Option<&Path>) -> Result<Entry, String> {
    if scope.len() > 80
        || !scope
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(crate::ui_text!(msg_dad54dd16204).into());
    }
    let (sid, descriptor) = identity()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    let name = format!("Local\\MediaBrowser-v1-{sid}-{scope}");
    let mutex_name = wide(std::ffi::OsStr::new(&name));
    let (mutex, exists) = unsafe {
        let h = CreateMutexW(Some(&attributes), false, PCWSTR(mutex_name.as_ptr()))
            .map_err(|e| e.to_string())?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        (Handle::new(h), exists)
    };
    let pipe_name = format!("\\\\.\\pipe\\MediaBrowser-v1-{sid}-{scope}");
    if exists {
        let request = Request {
            directory: directory.map(normalize).transpose()?,
        };
        let result = forward(&pipe_name, &request);
        drop(mutex);
        return result.map(|server_pid| Entry::Forwarded { server_pid });
    }
    let pipe_wide = wide(std::ffi::OsStr::new(&pipe_name));
    let pipe = unsafe {
        let handle = CreateNamedPipeW(
            PCWSTR(pipe_wide.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            MAX_FRAME as u32,
            MAX_FRAME as u32,
            1000,
            Some(&attributes),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(windows::core::Error::from_thread().to_string());
        }
        Handle::new(handle)
    };
    #[cfg(test)]
    let pipe_dacl = actual_dacl(pipe.raw())?;
    let stop = Arc::new(Handle::event()?);
    let stopped = stop.clone();
    let (sender, receiver) = mpsc::sync_channel(8);
    let wake: Wake = Arc::new(Mutex::new(None));
    let notify = wake.clone();
    let worker = thread::spawn(move || {
        let mut buffer = vec![0u8; MAX_FRAME];
        loop {
            if unsafe { WaitForSingleObject(stopped.raw(), 0) } == WAIT_OBJECT_0 {
                break;
            }
            if let Err(error) = connect(pipe.raw(), stopped.raw()) {
                if unsafe { WaitForSingleObject(stopped.raw(), 0) } != WAIT_OBJECT_0 {
                    eprintln!("{}", crate::ui_format!(msg_8e6fe7911f38, error = error));
                }
                break;
            }
            let result = (|| -> Result<(), String> {
                let n = read(pipe.raw(), stopped.raw(), &mut buffer, 1000)?;
                let request = decode(&buffer[..n]).and_then(|mut r| {
                    if let Some(path) = &r.directory {
                        if !path.is_absolute() {
                            return Err(crate::ui_text!(msg_497cf72c68a5).into());
                        }
                        r.directory = Some(normalize(path)?);
                    }
                    Ok(r)
                });
                let status = match request {
                    Ok(request) => match sender.try_send(request) {
                        Ok(()) => {
                            if let Some(w) = notify.lock().unwrap().clone() {
                                w();
                            }
                            1
                        }
                        Err(_) => 3,
                    },
                    Err(_) => 2,
                };
                write(pipe.raw(), stopped.raw(), &[status], 1000)?;
                let mut ack = [0u8; 1];
                let _ = read(pipe.raw(), stopped.raw(), &mut ack, 500);
                Ok(())
            })();
            let _ = result;
            unsafe {
                let _ = DisconnectNamedPipe(pipe.raw());
            }
        }
    });
    Ok(Entry::Primary(Primary {
        _mutex: mutex,
        stop,
        worker: Some(worker),
        receiver,
        wake,
        pipe_name,
        #[cfg(test)]
        pipe_dacl,
    }))
}

/// Reserve the normal application's instance name while changing installed files.
/// No IPC server is created: launches during installation fail instead of reporting
/// a successful directory handoff to an installer which cannot open it.
pub struct InstallationGuard {
    _handle: Handle,
}
pub fn installation_guard() -> Result<InstallationGuard, String> {
    let (sid, descriptor) = identity()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    let name = wide(std::ffi::OsStr::new(&format!(
        "Local\\MediaBrowser-v1-{sid}-main"
    )));
    unsafe {
        let handle = CreateMutexW(Some(&attributes), false, PCWSTR(name.as_ptr()))
            .map_err(|e| e.to_string())?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let handle = Handle::new(handle);
        if exists {
            return Err("请先关闭媒体浏览器及其他安装/卸载窗口，再重试".into());
        }
        Ok(InstallationGuard { _handle: handle })
    }
}
fn forward(pipe_name: &str, request: &Request) -> Result<u32, String> {
    let name = wide(std::ffi::OsStr::new(pipe_name));
    let deadline = Instant::now() + Duration::from_secs(4);
    let pipe = loop {
        match unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        } {
            Ok(h) => break unsafe { Handle::new(h) },
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(crate::ui_format!(msg_8760f5645048, e = e));
                }
                unsafe {
                    let _ = WaitNamedPipeW(PCWSTR(name.as_ptr()), 50);
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    };
    let mut pid = 0;
    unsafe {
        GetNamedPipeServerProcessId(pipe.raw(), &mut pid).map_err(|e| e.to_string())?;
        let _ = AllowSetForegroundWindow(pid);
        SetNamedPipeHandleState(pipe.raw(), Some(&PIPE_READMODE_MESSAGE), None, None)
            .map_err(|e| e.to_string())?;
    }
    let stop = Handle::event()?;
    write(pipe.raw(), stop.raw(), &encode(request)?, 2000)?;
    let mut response = [0u8; 1];
    let n = read(pipe.raw(), stop.raw(), &mut response, 2000)?;
    let _ = write(pipe.raw(), stop.raw(), &[1], 500);
    if n != 1 || response[0] != 1 {
        return Err(if response[0] == 3 {
            crate::ui_text!(msg_36911f4caaf6)
        } else {
            crate::ui_text!(msg_bc3eb8a3aac6)
        }
        .into());
    }
    Ok(pid)
}

#[cfg(test)]
fn actual_dacl(handle: HANDLE) -> Result<String, String> {
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            Some(&mut descriptor),
        )
        .ok()
        .map_err(|e| e.to_string())?;
        let descriptor = Descriptor(descriptor);
        let mut sddl = PWSTR::null();
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            1,
            DACL_SECURITY_INFORMATION,
            &mut sddl,
            None,
        )
        .map_err(|e| e.to_string())?;
        let text = sddl.to_string().map_err(|e| e.to_string());
        LocalFree(Some(HLOCAL(sddl.0.cast())));
        text
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    fn fixture() -> (String, PathBuf) {
        let id = format!(
            "test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(&id)
            .join("含 空格 🖼️");
        std::fs::create_dir_all(&path).unwrap();
        (id, path)
    }
    #[test]
    fn protocol_rejects_versions_lengths_nul_and_roundtrips_utf16() {
        let path = PathBuf::from(std::ffi::OsString::from_wide(&[
            67, 58, 92, 0xd800, 92, 0x56fe,
        ]));
        let request = Request {
            directory: Some(path),
        };
        assert_eq!(decode(&encode(&request).unwrap()).unwrap(), request);
        assert!(decode(b"arbitrary command").is_err());
        let mut frame = encode(&Request { directory: None }).unwrap();
        frame[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&frame).is_err());
        let mut frame = MAGIC.to_vec();
        frame.extend(2u32.to_le_bytes());
        frame.extend([0, 0]);
        assert!(decode(&frame).is_err());
        assert!(decode(&vec![0u8; MAX_FRAME + 1]).is_err());
    }
    #[test]
    fn native_same_user_pipe_acl_forward_bounded_queue_and_reopen() {
        let (scope, path) = fixture();
        let Entry::Primary(primary) = enter(&scope, None).unwrap() else {
            panic!("Expected primary")
        };
        let (sid, _) = identity().unwrap();
        for dacl in [
            &primary.pipe_dacl,
            &actual_dacl(primary._mutex.raw()).unwrap(),
        ] {
            assert_eq!(dacl.matches("(A;").count(), 1, "{dacl}");
            assert!(dacl.contains(&sid), "{dacl}");
            assert!(
                !dacl.contains(";;;WD)") && !dacl.contains(";;;BA)"),
                "{dacl}"
            );
        }
        for _ in 0..8 {
            let Entry::Forwarded { server_pid } = enter(&scope, Some(&path)).unwrap() else {
                panic!("Duplicate primary")
            };
            assert_eq!(server_pid, std::process::id());
        }
        assert!(
            enter(&scope, Some(&path))
                .unwrap_err_text()
                .contains("队列")
        );
        for _ in 0..8 {
            assert_eq!(
                primary.poll().unwrap().directory,
                Some(std::fs::canonicalize(&path).unwrap())
            );
        }
        assert!(primary.poll().is_none());
        let file = path.join("not-directory.jpg");
        std::fs::write(&file, b"fixture").unwrap();
        assert!(enter(&scope, Some(&file)).is_err());
        let Entry::Forwarded { .. } = enter(&scope, None).unwrap() else {
            panic!("Expected activation")
        };
        assert_eq!(primary.poll().unwrap().directory, None);
        let began = Instant::now();
        drop(primary);
        assert!(began.elapsed() < Duration::from_secs(1));
        let Entry::Primary(reopened) = enter(&scope, None).unwrap() else {
            panic!("Stale mutex")
        };
        drop(reopened);
    }
    trait ErrorText {
        fn unwrap_err_text(self) -> String;
    }
    impl ErrorText for Result<Entry, String> {
        fn unwrap_err_text(self) -> String {
            match self {
                Err(e) => e,
                Ok(_) => panic!("Expected error"),
            }
        }
    }
    #[test]
    fn silent_client_does_not_hold_server_shutdown() {
        let (scope, _) = fixture();
        let Entry::Primary(primary) = enter(&scope, None).unwrap() else {
            panic!("Expected primary")
        };
        let name = wide(std::ffi::OsStr::new(&primary.pipe_name));
        let client = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
            .map(|h| Handle::new(h))
            .unwrap()
        };
        thread::sleep(Duration::from_millis(30));
        let began = Instant::now();
        drop(primary);
        assert!(began.elapsed() < Duration::from_secs(1));
        drop(client);
    }
}
