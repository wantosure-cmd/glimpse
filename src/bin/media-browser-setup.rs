#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! Local-test offline installer. Payloads are appended by tools/package_local.py.
#[cfg(windows)]
mod setup {
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        os::windows::{fs::MetadataExt, process::CommandExt},
        path::{Component, Path, PathBuf},
        process::Command,
    };
    use windows::{
        Win32::{
            Foundation::*,
            System::{Registry::*, Threading::*},
        },
        core::PCWSTR,
    };
    const PRODUCT: &str = "media-browser-local-test-27194c79-v1";
    const MARKER: &str = "installation.json";
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LargeScreenMediaBrowser.LocalTest";
    #[derive(Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Entry {
        path: String,
        offset: u64,
        bytes: u64,
        sha256: String,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Manifest {
        product: String,
        files: Vec<Entry>,
        stub_bytes: u64,
        context_menu: bool,
        registered: bool,
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().chain(Some(0)).collect()
    }
    fn safe_relative(s: &str) -> bool {
        !s.is_empty()
            && !s.contains(['\\', ':', '\0'])
            && s.split('/')
                .all(|c| !c.is_empty() && !c.ends_with(['.', ' ']))
            && Path::new(s)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            && !matches!(s, MARKER | "uninstall.exe")
    }
    // Reject junctions/symlinks on every existing ancestor, not just the leaf.
    fn no_reparse(path: &Path) -> Result<(), String> {
        if !path.is_absolute() {
            return Err(media_browser::ui_text!(msg_31b9beb79f2d).into());
        }
        if path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(media_browser::ui_text!(msg_aaf8b25c5414).into());
        }
        for p in path.ancestors() {
            match fs::symlink_metadata(p) {
                Ok(m) if m.file_attributes() & 0x400 != 0 => {
                    return Err(media_browser::ui_format!(msg_69f740621c23,
                        p.display()
                    ));
                }
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    fn validate(m: &Manifest, limit: Option<u64>) -> Result<(), String> {
        if m.product != PRODUCT || m.files.is_empty() || m.files.len() > 10000 || m.stub_bytes == 0
        {
            return Err(media_browser::ui_text!(msg_7eccd5b6a756).into());
        }
        let mut names = std::collections::HashSet::new();
        let mut end = m.stub_bytes;
        for e in &m.files {
            if !safe_relative(&e.path)
                || !names.insert(e.path.to_ascii_lowercase())
                || e.sha256.len() != 64
                || !e.sha256.bytes().all(|c| c.is_ascii_hexdigit())
                || e.offset != end
            {
                return Err(media_browser::ui_text!(msg_e2c5c9bc4478).into());
            }
            end = end
                .checked_add(e.bytes)
                .ok_or(media_browser::ui_text!(msg_fe1d75aabde1))?;
        }
        if limit.is_some_and(|l| l != end) {
            return Err(media_browser::ui_text!(msg_2b117f8f0d0b).into());
        }
        Ok(())
    }
    fn payload(exe: &Path) -> Result<(File, Manifest), String> {
        let mut f = File::open(exe).map_err(|e| e.to_string())?;
        let len = f.metadata().map_err(|e| e.to_string())?.len();
        if len < 24 {
            return Err(media_browser::ui_text!(msg_a67ec7210f65).into());
        }
        f.seek(SeekFrom::End(-24)).map_err(|e| e.to_string())?;
        let mut tail = [0; 24];
        f.read_exact(&mut tail).map_err(|e| e.to_string())?;
        if &tail[..8] != b"MBSET001" {
            return Err(media_browser::ui_text!(msg_d2b92c353e5e).into());
        }
        let n = u64::from_le_bytes(tail[8..16].try_into().unwrap());
        let stub = u64::from_le_bytes(tail[16..].try_into().unwrap());
        if n > 4 * 1024 * 1024 || n > len - 24 {
            return Err(media_browser::ui_text!(msg_dd426b049600).into());
        }
        let start = len - 24 - n;
        f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        let mut data = vec![0; n as usize];
        f.read_exact(&mut data).map_err(|e| e.to_string())?;
        let m: Manifest = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        if m.stub_bytes != stub {
            return Err(media_browser::ui_text!(msg_e37c51351f93).into());
        }
        validate(&m, Some(start))?;
        Ok((f, m))
    }
    fn digest(path: &Path) -> Result<String, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buf = vec![0; 1024 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    fn set_registry(root: &Path, remove: bool) -> Result<(), String> {
        let name = wide(std::ffi::OsStr::new(KEY));
        let mut key = HKEY::default();
        unsafe {
            if remove {
                // The caller has already validated the product marker and install root.
                // Also verify this exact install owns the uninstall registration.
                let rc = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(name.as_ptr()),
                    None,
                    KEY_READ,
                    &mut key,
                );
                if rc == ERROR_FILE_NOT_FOUND {
                    return Ok(());
                }
                rc.ok().map_err(|e| e.to_string())?;
                let value = wide(std::ffi::OsStr::new("InstallLocation"));
                let mut buffer = vec![0u8; 65536];
                let mut len = buffer.len() as u32;
                let mut kind = REG_VALUE_TYPE::default();
                let rc = RegQueryValueExW(
                    key,
                    PCWSTR(value.as_ptr()),
                    None,
                    Some(&mut kind),
                    Some(buffer.as_mut_ptr()),
                    Some(&mut len),
                );
                let _ = RegCloseKey(key);
                rc.ok().map_err(|e| e.to_string())?;
                if kind != REG_SZ || len % 2 != 0 {
                    return Err(media_browser::ui_text!(msg_94f409cf1fdd).into());
                }
                let units: Vec<u16> = buffer[..len as usize]
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .take_while(|c| *c != 0)
                    .collect();
                if PathBuf::from(String::from_utf16(&units).map_err(|e| e.to_string())?) != root {
                    return Ok(());
                }
                RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(name.as_ptr()))
                    .ok()
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
            // Never replace another installation's registration implicitly.
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(name.as_ptr()),
                None,
                KEY_READ,
                &mut key,
            )
            .is_ok()
            {
                let _ = RegCloseKey(key);
                return Err(media_browser::ui_text!(msg_71c59bbfe015).into());
            }
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(name.as_ptr()),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()
            .map_err(|e| e.to_string())?;
            let entries = [
                (
                    "DisplayName",
                    media_browser::ui_text!(msg_eb5ddfceb467).to_owned(),
                ),
                ("DisplayVersion", env!("CARGO_PKG_VERSION").to_owned()),
                ("InstallLocation", root.to_string_lossy().into_owned()),
                (
                    "UninstallString",
                    format!("\"{}\" --uninstall", root.join("uninstall.exe").display()),
                ),
            ];
            let result = (|| {
                for (n, v) in entries {
                    let n = wide(std::ffi::OsStr::new(n));
                    let bytes: Vec<u8> = wide(std::ffi::OsStr::new(&v))
                        .into_iter()
                        .flat_map(u16::to_le_bytes)
                        .collect();
                    RegSetValueExW(key, PCWSTR(n.as_ptr()), None, REG_SZ, Some(&bytes))
                        .ok()
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })();
            let _ = RegCloseKey(key);
            result
        }
    }
    fn install(
        exe: &Path,
        root: &Path,
        context_menu: bool,
        registered: bool,
    ) -> Result<(), String> {
        let _guard = media_browser::instance::installation_guard()?;
        let (mut source, mut m) = payload(exe)?;
        no_reparse(root)?;
        if root.exists()
            && fs::read_dir(root)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err(media_browser::ui_text!(msg_d292eee4451c).into());
        }
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        m.context_menu = false;
        m.registered = false;
        // Write the recovery manifest first. Even interrupted installs can be removed.
        let marker = root.join(MARKER);
        let save = |m: &Manifest| -> Result<(), String> {
            fs::write(
                &marker,
                serde_json::to_vec_pretty(m).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        };
        save(&m)?;
        let mut uninstaller = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("uninstall.exe"))
            .map_err(|e| e.to_string())?;
        source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut (&mut source).take(m.stub_bytes), &mut uninstaller)
            .map_err(|e| e.to_string())?;
        if copied != m.stub_bytes {
            return Err(media_browser::ui_text!(msg_b4b0cd771e17).into());
        }
        drop(uninstaller);
        let mut buffer = vec![0; 1024 * 1024];
        for entry in &m.files {
            let out = root.join(&entry.path);
            no_reparse(&out)?;
            fs::create_dir_all(
                out.parent()
                    .ok_or(media_browser::ui_text!(msg_171320732419))?,
            )
            .map_err(|e| e.to_string())?;
            let mut target = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(|e| e.to_string())?;
            source
                .seek(SeekFrom::Start(entry.offset))
                .map_err(|e| e.to_string())?;
            let mut remaining = entry.bytes;
            let mut hash = Sha256::new();
            while remaining > 0 {
                let n = remaining.min(buffer.len() as u64) as usize;
                source
                    .read_exact(&mut buffer[..n])
                    .map_err(|e| e.to_string())?;
                target.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                hash.update(&buffer[..n]);
                remaining -= n as u64;
            }
            target.sync_all().map_err(|e| e.to_string())?;
            if format!("{:x}", hash.finalize()) != entry.sha256 {
                return Err(media_browser::ui_format!(msg_12ac46630d1c,
                    entry.path
                ));
            }
        }
        if registered {
            set_registry(root, false)?;
            m.registered = true;
            save(&m)?;
        }
        if context_menu {
            media_browser::shell_menu::register(&root.join("media-browser.exe"), None)?;
            m.context_menu = true;
            save(&m)?;
        }
        Ok(())
    }
    fn uninstall(root: &Path) -> Result<(), String> {
        let _guard = media_browser::instance::installation_guard()?;
        no_reparse(root)?;
        let marker = root.join(MARKER);
        no_reparse(&marker)?;
        let data = fs::read(&marker).map_err(|e| e.to_string())?;
        if data.len() > 4 * 1024 * 1024 {
            return Err(media_browser::ui_text!(msg_94dbf0c6b2ce).into());
        }
        let m: Manifest = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        validate(&m, None)?;
        // Preflight the whole tree and every owned file before any removal. Changed
        // files are retained; no recursive deletion and no permanent media deletion.
        let mut files = Vec::new();
        let mut retained = Vec::new();
        for e in &m.files {
            let p = root.join(&e.path);
            no_reparse(&p)?;
            if p.exists() {
                if !p.is_file() {
                    return Err(media_browser::ui_text!(msg_7c447c337aea).into());
                }
                if digest(&p)? == e.sha256 {
                    files.push(p);
                } else {
                    retained.push(e.path.clone());
                }
            }
        }
        let app = root.join("media-browser.exe");
        // Diagnostic instances may deliberately bypass the main named mutex. Check
        // the executable's OS sharing restriction too before removing any file.
        if app.exists() {
            let _probe = OpenOptions::new()
                .write(true)
                .open(&app)
                .map_err(|e| media_browser::ui_format!(msg_3d9ecb53e93e, e = e))?;
        }
        if m.context_menu && app.exists() {
            media_browser::shell_menu::unregister(&app, None)?;
        }
        if m.registered {
            set_registry(root, true)?;
        }
        let mut dirs = std::collections::BTreeSet::new();
        for p in files {
            no_reparse(&p)?;
            fs::remove_file(&p).map_err(|e| {
                media_browser::ui_format!(msg_be1eccaf8619,
                    p.display(),
                    e = e
                )
            })?;
            let mut dir = p.parent();
            while let Some(d) = dir {
                if d == root {
                    break;
                }
                dirs.insert(d.to_owned());
                dir = d.parent();
            }
        }
        for d in dirs.iter().rev() {
            let _ = fs::remove_dir(d);
        }
        no_reparse(&root.join("uninstall.exe"))?;
        fs::remove_file(root.join("uninstall.exe")).map_err(|e| e.to_string())?;
        fs::remove_file(marker).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir(root);
        if !retained.is_empty() {
            println!("{}", media_browser::ui_format!(msg_e1d7690cc9c0,
                retained.join(", ")
            ));
        }
        Ok(())
    }
    fn question(text: &str) -> bool {
        rfd::MessageDialog::new()
            .set_title(media_browser::ui_text!(msg_0571bfb0a16a))
            .set_description(text)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes
    }
    pub fn run() -> Result<(), String> {
        let args: Vec<_> = std::env::args_os().collect();
        let has = |s: &str| args.iter().any(|a| a == s);
        let arg = |s: &str| {
            args.iter()
                .position(|a| a == s)
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
        };
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let silent = has("--silent");
        if let Some(root) = arg("--cleanup-root") {
            let pid = arg("--wait-pid")
                .and_then(|s| s.to_str()?.parse::<u32>().ok())
                .ok_or(media_browser::ui_text!(msg_3d08510963d4))?;
            unsafe {
                match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                    Ok(process) => {
                        let result = WaitForSingleObject(process, 15000);
                        let _ = CloseHandle(process);
                        if result != WAIT_OBJECT_0 {
                            return Err(media_browser::ui_text!(msg_9c2975ca359d).into());
                        }
                    }
                    // The parent can finish before the child is scheduled.
                    Err(e)
                        if e.code()
                            == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
                    {
                        ()
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            uninstall(&root)?;
        } else if has("--uninstall") {
            let root = exe
                .parent()
                .ok_or(media_browser::ui_text!(msg_22f394232b53))?;
            if !silent && !question(media_browser::ui_text!(msg_db0276bc25d9)) {
                return Ok(());
            }
            let temp = std::env::temp_dir().join(format!(
                "media-browser-uninstall-{}-{}.exe",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ));
            // create_new avoids overwriting a pre-existing temporary executable.
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|e| e.to_string())?;
            std::io::copy(&mut File::open(&exe).map_err(|e| e.to_string())?, &mut out)
                .map_err(|e| e.to_string())?;
            drop(out);
            let mut cmd = Command::new(&temp);
            cmd.args([
                std::ffi::OsStr::new("--cleanup-root"),
                root.as_os_str(),
                std::ffi::OsStr::new("--wait-pid"),
                std::ffi::OsStr::new(&std::process::id().to_string()),
            ]);
            if silent {
                cmd.arg("--silent");
            }
            cmd.creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        } else {
            let root = arg("--install-dir").unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                    .join("Programs/LargeScreenMediaBrowser-Test")
            });
            if !silent
                && !question(&media_browser::ui_format!(msg_bd1a7b3c6e92,
                    root.display()
                ))
            {
                return Ok(());
            }
            let menu = has("--context-menu")
                || (!silent && question(media_browser::ui_text!(msg_0db5bc6c9681)));
            install(&exe, &root, menu, !has("--no-register"))?;
        }
        if !silent {
            rfd::MessageDialog::new()
                .set_title(media_browser::ui_text!(msg_81793ece95dd))
                .set_description(media_browser::ui_text!(msg_2704a42e22dc))
                .show();
        }
        Ok(())
    }
    pub fn error(message: &str) {
        rfd::MessageDialog::new()
            .set_title(media_browser::ui_text!(msg_f2d8cdd0672a))
            .set_description(message)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn rejects_traversal_reserved_payloads_and_noncontiguous_ranges() {
            for p in [
                "../a",
                "/a",
                "C:/a",
                "runtime/../a",
                "runtime\\a",
                "a//b",
                "a/./b",
                "a/ ",
                "installation.json",
                "uninstall.exe",
                "a:stream",
            ] {
                assert!(!safe_relative(p), "{p}");
            }
            assert!(safe_relative("licenses/中文 文件.txt"));
            let mut m = Manifest {
                product: PRODUCT.into(),
                stub_bytes: 10,
                context_menu: false,
                registered: false,
                files: vec![Entry {
                    path: "media-browser.exe".into(),
                    offset: 10,
                    bytes: 20,
                    sha256: "a".repeat(64),
                }],
            };
            assert!(validate(&m, Some(30)).is_ok());
            assert!(validate(&m, Some(31)).is_err());
            m.files.push(m.files[0].clone());
            assert!(validate(&m, None).is_err());
            m.files.pop();
            m.files[0].offset = 9;
            assert!(validate(&m, None).is_err());
        }
    }
}
fn main() {
    #[cfg(windows)]
    if let Err(e) = setup::run() {
        eprintln!("{e}");
        if !std::env::args_os().any(|s| s == "--silent") {
            setup::error(&e);
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("This installer targets Windows only.");
        std::process::exit(1);
    }
}
