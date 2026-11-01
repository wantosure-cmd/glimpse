//! Current-user static verbs only. No default associations, services or HKLM writes.
use serde::Serialize;
use std::{collections::BTreeMap, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{Foundation::*, System::Registry::*, UI::Shell::*},
    core::PCWSTR,
};
const OWNER: &str = "media-browser-27194c79-6f27-4b85-b686-664e6de2ab40";
const VALUES: [&str; 5] = [
    "MediaBrowserOwner",
    "MediaBrowserExecutable",
    "MUIVerb",
    "Icon",
    "MultiSelectModel",
];
fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(Some(0))
        .collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
impl Key {
    fn open(path: &str) -> Result<Option<Self>, String> {
        let path = wide(path);
        let mut key = HKEY::default();
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                KEY_READ | KEY_WRITE,
                &mut key,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        code.ok().map_err(|e| e.to_string())?;
        Ok(Some(Self(key)))
    }
    fn create(path: &str) -> Result<Self, String> {
        let path = wide(path);
        let mut key = HKEY::default();
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()
            .map_err(|e| e.to_string())?;
        }
        Ok(Self(key))
    }
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        let name = wide(name);
        let mut size = 0;
        let mut kind = REG_VALUE_TYPE::default();
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        code.ok().map_err(|e| e.to_string())?;
        if kind != REG_SZ || size > 128 * 1024 || size % 2 != 0 {
            return Err(crate::ui_text!(msg_b88d784d44a0).into());
        }
        let mut bytes = vec![0u8; size as usize];
        unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
            .ok()
            .map_err(|e| e.to_string())?;
        }
        let units: Vec<u16> = bytes[..size as usize]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take_while(|c| *c != 0)
            .collect();
        String::from_utf16(&units)
            .map(Some)
            .map_err(|e| e.to_string())
    }
    fn set(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        let name = wide(name);
        let code = if let Some(value) = value {
            let bytes: Vec<u8> = wide(value).into_iter().flat_map(u16::to_le_bytes).collect();
            unsafe { RegSetValueExW(self.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes)) }
        } else {
            unsafe { RegDeleteValueW(self.0, PCWSTR(name.as_ptr())) }
        };
        if value.is_none() && code == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        code.ok().map_err(|e| e.to_string())
    }
}
fn remove(path: &str) -> Result<(), String> {
    let path = wide(path);
    let code = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    code.ok().map_err(|e| e.to_string())
}
fn executable(path: &Path) -> Result<String, String> {
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Err(crate::ui_text!(msg_a6c8d44312d2).into());
    }
    let text = path.to_str().ok_or(crate::ui_text!(msg_59241e82ee02))?;
    let text = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(text).to_owned()
    };
    if text.contains(['"', '\0']) {
        return Err(crate::ui_text!(msg_27f1f755b80d).into());
    }
    Ok(text)
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub registry_path: String,
    pub command: String,
    pub executable: String,
}
fn plans(base: &str, exe: &Path, test_scope: Option<&str>) -> Result<Vec<Entry>, String> {
    if test_scope.is_some_and(|s| {
        s.is_empty() || s.len() > 80 || !s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    }) {
        return Err(crate::ui_text!(msg_54a39cce49c2).into());
    }
    let executable = executable(exe)?;
    let verb = test_scope
        .map(|s| format!("MediaBrowser.Test-{s}"))
        .unwrap_or_else(|| "MediaBrowser".into());
    let extra = test_scope
        .map(|s| format!(" --instance-scope {s}"))
        .unwrap_or_default();
    // The trailing \. prevents a drive-root's final backslash from escaping the
    // closing quote in Windows argv parsing. Both normal folders and roots normalize.
    Ok([
        ("Directory", "%1"),
        ("Directory\\Background", "%V"),
        ("Drive", "%1"),
    ]
    .into_iter()
    .map(|(class, placeholder)| Entry {
        registry_path: format!("{base}\\{class}\\shell\\{verb}"),
        command: format!("\"{executable}\"{extra} --open \"{placeholder}\\.\""),
        executable: executable.clone(),
    })
    .collect())
}
struct Snapshot {
    path: String,
    existed: bool,
    values: BTreeMap<String, Option<String>>,
    command: Option<Option<String>>,
}
fn snapshot(entry: &Entry) -> Result<Snapshot, String> {
    let key = Key::open(&entry.registry_path)?;
    if let Some(key) = &key {
        if key.get("MediaBrowserOwner")?.as_deref() != Some(OWNER) {
            return Err(crate::ui_format!(msg_ea64aefa8c7e,
                entry.registry_path
            ));
        }
    }
    let values = VALUES
        .into_iter()
        .map(|name| {
            Ok((
                name.to_owned(),
                if let Some(key) = &key {
                    key.get(name)?
                } else {
                    None
                },
            ))
        })
        .collect::<Result<_, String>>()?;
    let command = Key::open(&format!("{}\\command", entry.registry_path))?
        .map(|k| k.get(""))
        .transpose()?;
    Ok(Snapshot {
        path: entry.registry_path.clone(),
        existed: key.is_some(),
        values,
        command,
    })
}
fn rollback(old: &Snapshot) -> Result<(), String> {
    if !old.existed {
        return remove(&old.path);
    }
    let key = Key::create(&old.path)?;
    for (name, value) in &old.values {
        key.set(name, value.as_deref())?;
    }
    let command = format!("{}\\command", old.path);
    if let Some(value) = &old.command {
        Key::create(&command)?.set("", value.as_deref())?;
    } else {
        remove(&command)?;
    }
    Ok(())
}
fn install(entries: &[Entry], test: bool) -> Result<(), String> {
    let snapshots = entries
        .iter()
        .map(snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    let result = (|| -> Result<(), String> {
        for entry in entries {
            let key = Key::create(&entry.registry_path)?;
            key.set("MediaBrowserOwner", Some(OWNER))?;
            key.set("MediaBrowserExecutable", Some(&entry.executable))?;
            key.set(
                "MUIVerb",
                Some(if test {
                    crate::ui_text!(msg_7aab95bb682d)
                } else {
                    crate::ui_text!(msg_7a723bf1350b)
                }),
            )?;
            key.set("Icon", Some(&format!("\"{}\",0", entry.executable)))?;
            key.set("MultiSelectModel", Some("Single"))?;
            Key::create(&format!("{}\\command", entry.registry_path))?
                .set("", Some(&entry.command))?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let failures: Vec<_> = snapshots
            .iter()
            .rev()
            .filter_map(|old| rollback(old).err())
            .collect();
        return Err(crate::ui_format!(msg_7199aff8544d,
            failures.join("；"),
            error = error
        ));
    }
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct Removal {
    pub removed: Vec<String>,
    pub preserved: Vec<String>,
}
fn uninstall(entries: &[Entry]) -> Result<Removal, String> {
    let mut report = Removal {
        removed: vec![],
        preserved: vec![],
    };
    for entry in entries {
        let Some(key) = Key::open(&entry.registry_path)? else {
            continue;
        };
        let owner = key.get("MediaBrowserOwner")?;
        let exe = key.get("MediaBrowserExecutable")?;
        drop(key);
        if owner.as_deref() != Some(OWNER) || exe.as_deref() != Some(&entry.executable) {
            report.preserved.push(entry.registry_path.clone());
            continue;
        }
        remove(&entry.registry_path)?;
        report.removed.push(entry.registry_path.clone());
    }
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(report)
}
pub fn registered(exe: &Path) -> Result<bool, String> {
    for entry in plans("Software\\Classes", exe, None)? {
        let Some(key) = Key::open(&entry.registry_path)? else {
            return Ok(false);
        };
        if key.get("MediaBrowserOwner")?.as_deref() != Some(OWNER)
            || key.get("MediaBrowserExecutable")?.as_deref() != Some(&entry.executable)
        {
            return Ok(false);
        }
        let Some(command) = Key::open(&format!("{}\\command", entry.registry_path))? else {
            return Ok(false);
        };
        if command.get("")?.as_deref() != Some(&entry.command) {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn register(exe: &Path, test_scope: Option<&str>) -> Result<Vec<Entry>, String> {
    let entries = plans("Software\\Classes", exe, test_scope)?;
    install(&entries, test_scope.is_some())?;
    Ok(entries)
}
pub fn unregister(exe: &Path, test_scope: Option<&str>) -> Result<Removal, String> {
    uninstall(&plans("Software\\Classes", exe, test_scope)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owned() -> (String, std::path::PathBuf) {
        let id = format!(
            "test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(&id)
            .join("含 空格");
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("媒体 Browser.exe");
        std::fs::write(&exe, b"test fixture, never executed").unwrap();
        (format!("Software\\MediaBrowserTests\\{id}\\Classes"), exe)
    }
    #[test]
    fn registry_ownership_upgrade_uninstall_preserves_other_install_and_unknown_verbs() {
        let (base, exe) = owned();
        let entries = plans(&base, &exe, None).unwrap();
        install(&entries, false).unwrap();
        install(&entries, false).unwrap();
        for e in &entries {
            assert_eq!(
                Key::open(&format!("{}\\command", e.registry_path))
                    .unwrap()
                    .unwrap()
                    .get("")
                    .unwrap()
                    .as_deref(),
                Some(e.command.as_str())
            );
        }
        let other = exe.with_file_name("new.exe");
        std::fs::write(&other, b"different install").unwrap();
        let newer = plans(&base, &other, None).unwrap();
        install(&newer, false).unwrap();
        let report = uninstall(&entries).unwrap();
        assert_eq!(report.preserved.len(), 3);
        assert!(report.removed.is_empty());
        let report = uninstall(&newer).unwrap();
        assert_eq!(report.removed.len(), 3);
        for e in &newer {
            assert!(Key::open(&e.registry_path).unwrap().is_none());
        }
        assert!(uninstall(&newer).unwrap().removed.is_empty());
        let key = Key::create(&entries[1].registry_path).unwrap();
        key.set("MediaBrowserOwner", Some("another-app")).unwrap();
        key.set("Keep", Some("unchanged")).unwrap();
        drop(key);
        assert!(install(&entries, false).is_err());
        assert!(Key::open(&entries[0].registry_path).unwrap().is_none());
        assert_eq!(
            Key::open(&entries[1].registry_path)
                .unwrap()
                .unwrap()
                .get("Keep")
                .unwrap()
                .as_deref(),
            Some("unchanged")
        );
        assert_eq!(uninstall(&entries).unwrap().preserved.len(), 1);
        assert!(base.starts_with("Software\\MediaBrowserTests\\test-"));
        remove(base.strip_suffix("\\Classes").unwrap()).unwrap();
    }
    #[test]
    fn command_preserves_quoted_executable_and_drive_root_directory_arguments() {
        let (base, exe) = owned();
        let entries = plans(&base, &exe, Some("test-scope")).unwrap();
        for entry in entries {
            for folder in ["D:\\", "D:\\含 空格\\图集"] {
                let text = entry.command.replace("%1", folder).replace("%V", folder);
                let text = wide(&text);
                let mut count = 0;
                let argv = unsafe { CommandLineToArgvW(PCWSTR(text.as_ptr()), &mut count) };
                assert!(!argv.is_null());
                let args = unsafe { std::slice::from_raw_parts(argv, count as usize) }
                    .iter()
                    .map(|p| unsafe { p.to_string().unwrap() })
                    .collect::<Vec<_>>();
                unsafe {
                    LocalFree(Some(HLOCAL(argv.cast())));
                }
                assert_eq!(args[0], executable(&exe).unwrap());
                assert_eq!(&args[1..4], ["--instance-scope", "test-scope", "--open"]);
                assert_eq!(args[4], format!("{folder}\\."));
            }
        }
    }
}
