//! Windows-only, narrowly owned Codex hooks.json installation.
//! config.toml (including security settings and trust) is never rewritten.
use crate::hooks::{HookPreview, HookStatus};
use crate::{platform, settings};
use base64::Engine;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const OWNER: &str = "Coucou Codex activity (Windows)";
const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "Interrupt",
    "SessionEnd",
];

fn path() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| platform::home_dir().join(".codex"))
        .join("hooks.json")
}
fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}
fn parse(bytes: Option<&[u8]>) -> Result<Value, String> {
    let bytes = bytes.unwrap_or(b"{}");
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "Invalid Codex hooks.json; nothing was written.")?;
    if !value.is_object() {
        return Err("Codex hooks.json must be an object.".into());
    }
    if let Some(hooks) = value.get("hooks") {
        let hooks = hooks.as_object().ok_or("Codex hooks must be an object.")?;
        for groups in hooks.values() {
            let groups = groups
                .as_array()
                .ok_or("Codex event hooks must be arrays.")?;
            for group in groups {
                if !group.is_object() || !group.get("hooks").is_some_and(Value::is_array) {
                    return Err("Invalid Codex matcher group; nothing was written.".into());
                }
            }
        }
    }
    Ok(value)
}
fn owned(handler: &Value) -> bool {
    handler.get("statusMessage").and_then(Value::as_str) == Some(OWNER)
        && handler.get("type").and_then(Value::as_str) == Some("command")
        && handler
            .get("commandWindows")
            .and_then(Value::as_str)
            .is_some_and(|s| {
                s.starts_with("powershell.exe -NoProfile -NonInteractive -EncodedCommand ")
            })
}
fn transform(value: &Value, install: bool, exe: &Path) -> Result<Value, String> {
    let mut next = value.clone();
    if next.get("hooks").is_none() {
        if !install {
            return Ok(next);
        }
        next["hooks"] = json!({});
    }
    let hooks = next["hooks"]
        .as_object_mut()
        .ok_or("Invalid hooks object")?;
    // Remove only our handler, retaining foreign handlers in the same group.
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        groups.retain_mut(|group| {
            let foreign_metadata = group
                .as_object()
                .is_some_and(|fields| fields.keys().any(|key| key != "hooks"));
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let had_ours = handlers.iter().any(owned);
            handlers.retain(|h| !owned(h));
            !had_ours || !handlers.is_empty() || foreign_metadata
        });
    }
    if install {
        let exe = exe.to_string_lossy();
        // Codex may select cmd or PowerShell. An encoded PowerShell command
        // keeps %, &, $, backticks and quotes in paths out of the outer shell.
        let script = format!("& '{}' --agent codex", exe.replace('\'', "''"));
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let command = format!(
            "powershell.exe -NoProfile -NonInteractive -EncodedCommand {}",
            base64::engine::general_purpose::STANDARD.encode(utf16)
        );
        for event in EVENTS {
            let groups = hooks.entry(event.to_string()).or_insert(json!([]));
            let groups = groups.as_array_mut().ok_or("Invalid event hooks")?;
            groups.push(json!({"hooks":[{
                "type":"command", "command":"exit 0", "commandWindows":command,
                "timeout":3, "statusMessage":OWNER
            }]}));
        }
    }
    Ok(next)
}
fn fingerprint(bytes: Option<&[u8]>) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for b in bytes.unwrap_or(b"\0absent") {
        hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
fn backup(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        "hooks.json.coucou-backup-{nanos}-{}",
        std::process::id()
    ))
}
pub fn status() -> HookStatus {
    let path = path();
    let installed = read(&path)
        .ok()
        .and_then(|b| parse(b.as_deref()).ok())
        .and_then(|v| v.get("hooks").cloned())
        .and_then(|v| v.as_object().cloned())
        .is_some_and(|hooks| {
            hooks
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(|g| g.get("hooks").and_then(Value::as_array))
                .flatten()
                .any(owned)
        });
    HookStatus {
        installed,
        settings_path: path.display().to_string(),
        hook_path: settings::hook_exe_path().display().to_string(),
        hook_ready: settings::hook_exe_path().is_file(),
    }
}
fn inspect_config(hooks_path: &Path) -> Result<String, String> {
    let config = hooks_path.with_file_name("config.toml");
    let Some(bytes) = read(&config)? else {
        return Ok("No user config.toml exists; Codex defaults apply.".into());
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "Cannot decode Codex config.toml; it was not changed.")?;
    let value: toml::Value =
        toml::from_str(text).map_err(|_| "Cannot parse Codex config.toml; it was not changed.")?;
    let disabled = value
        .get("features")
        .and_then(|v| v.get("hooks").or_else(|| v.get("codex_hooks")))
        .and_then(toml::Value::as_bool)
        == Some(false);
    Ok(if disabled { "User config.toml disables hooks. Coucou will preserve that choice; no events will arrive until you enable hooks yourself." }
        else { "Existing config.toml inspected and preserved. Inline hooks also remain active; Codex hook trust and managed policy still apply." }.into())
}
pub fn preview(install: bool) -> Result<HookPreview, String> {
    let path = path();
    let bytes = read(&path)?;
    let value = parse(bytes.as_deref())?;
    let config_note = inspect_config(&path)?;
    let next = transform(&value, install, &settings::hook_exe_path())?;
    // A hooks file may contain credentials in somebody else's commands. Never
    // include those commands (or config.toml contents) in our preview.
    let diff = if value == next {
        "No change.".into()
    } else {
        format!("{} Coucou-owned Codex activity handlers for:\n{}\n\nOther settings and handlers are preserved. config.toml is untouched.\nNo PermissionRequest hook is installed. Review/trust the hooks in Codex /hooks before use.",
            if install { "Install/update" } else { "Remove" }, EVENTS.join(", "))
    };
    let diff = format!("{diff}\n\n{config_note}");
    Ok(HookPreview {
        diff,
        backup: backup(&path).display().to_string(),
        settings_path: path.display().to_string(),
        fingerprint: fingerprint(bytes.as_deref()),
    })
}
pub fn write(install: bool, expected: &str) -> Result<String, String> {
    let path = path();
    inspect_config(&path)?;
    write_at(&path, &settings::hook_exe_path(), install, expected)
}
fn write_at(path: &Path, exe: &Path, install: bool, expected: &str) -> Result<String, String> {
    let bytes = read(path)?;
    let current = parse(bytes.as_deref())?;
    if fingerprint(bytes.as_deref()) != expected {
        return Err("Codex hooks changed since the preview. Review again.".into());
    }
    let next = transform(&current, install, exe)?;
    if next == current {
        return Ok("No change; no backup needed".into());
    }
    if install && !exe.is_file() {
        return Err("Coucou relay is missing. Restart Coucou before installing.".into());
    }
    std::fs::create_dir_all(path.parent().ok_or("Invalid hooks path")?)
        .map_err(|e| e.to_string())?;
    // Preserve a user's dotfile link, and never silently replace a reparse point.
    if std::fs::symlink_metadata(path).is_ok_and(|m| {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }) {
        return Err(
            "Codex hooks.json is a reparse point; install manually without replacing it.".into(),
        );
    }
    let backup = backup(path);
    if let Some(original) = &bytes {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .map_err(|e| e.to_string())?;
        file.write_all(original)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let temp = backup.with_extension("new");
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let mut text = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
        text.push(b'\n');
        file.write_all(&text)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        // Refuse edits made while preparing the backup/temp too.
        if read(path)? != bytes {
            return Err("Codex hooks changed during installation; nothing was replaced.".into());
        }
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    Ok(if bytes.is_some() {
        backup.display().to_string()
    } else {
        "New hooks.json; no previous file".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_reinstall_uninstall_preserve_foreign_handlers() {
        let original = json!({"description":"mine","extra":{"keep":true},"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"mine"}]}],"FutureEvent":[]}});
        let installed = transform(&original, true, Path::new("C:/a b/coucou-hook.exe")).unwrap();
        assert_eq!(
            transform(&installed, true, Path::new("C:/a b/coucou-hook.exe")).unwrap(),
            installed
        );
        let mut shared = installed.clone();
        shared["hooks"]["Stop"][0]["hooks"]
            .as_array_mut()
            .unwrap()
            .push(json!({"command":"foreign"}));
        shared["hooks"]["Interrupt"][0]["user_metadata"] = json!({"preserve":true});
        let removed = transform(&shared, false, Path::new("ignored")).unwrap();
        assert_eq!(removed["extra"], original["extra"]);
        assert_eq!(
            removed["hooks"]["PreToolUse"],
            original["hooks"]["PreToolUse"]
        );
        assert_eq!(
            removed["hooks"]["Stop"][0]["hooks"][0]["command"],
            "foreign"
        );
        assert_eq!(
            removed["hooks"]["Interrupt"][0]["user_metadata"],
            json!({"preserve":true})
        );
        assert_eq!(removed["hooks"]["Interrupt"][0]["hooks"], json!([]));
        assert_eq!(
            transform(&removed, false, Path::new("ignored")).unwrap(),
            removed
        );
        assert!(installed["hooks"].get("PermissionRequest").is_none());
    }
    #[test]
    fn refuses_malformed_config_and_keeps_bom_support() {
        for bad in [
            b"{ broken".as_slice(),
            b"[]",
            br#"{"hooks":null}"#,
            br#"{"hooks":{"Stop":"bad"}}"#,
        ] {
            assert!(parse(Some(bad)).is_err());
        }
        assert_eq!(parse(Some(b"\xef\xbb\xbf{}")).unwrap(), json!({}));
        assert_ne!(fingerprint(None), fingerprint(Some(b"")));
    }
    #[test]
    fn quote_windows_paths_literally() {
        let v = transform(&json!({}), true, Path::new("C:/a' $`&/coucou-hook.exe")).unwrap();
        let command = v["hooks"]["Stop"][0]["hooks"][0]["commandWindows"]
            .as_str()
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(command.split_whitespace().last().unwrap())
            .unwrap();
        let chars: Vec<u16> = bytes
            .chunks(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(
            String::from_utf16(&chars).unwrap(),
            "& 'C:/a'' $`&/coucou-hook.exe' --agent codex"
        );
    }
    #[test]
    fn inspect_config_preserves_disabled_hooks_and_inline_configuration() {
        let dir =
            std::env::temp_dir().join(format!("coucou-codex-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.toml");
        let text="model = \"existing-model\"\n[features]\nhooks = false\n[hooks.state]\n[env]\nprivate_example = \"never-display-me\"\n";
        std::fs::write(&config, text).unwrap();
        let note = inspect_config(&dir.join("hooks.json")).unwrap();
        assert!(note.contains("disables hooks"));
        assert!(!note.contains("never-display-me"));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
        std::fs::write(&config, b"[bad").unwrap();
        assert!(inspect_config(&dir.join("hooks.json")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn disk_backup_stale_refusal_and_idempotence() {
        let dir = std::env::temp_dir().join(format!("coucou-codex-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hooks.json");
        let exe = dir.join("coucou-hook.exe");
        std::fs::write(&exe, b"test relay").unwrap();
        let original =
            br#"{"description":"preserve","hooks":{"Stop":[{"hooks":[{"command":"foreign"}]}]}}"#;
        std::fs::write(&path, original).unwrap();
        let saved = write_at(&path, &exe, true, &fingerprint(Some(original))).unwrap();
        assert_eq!(std::fs::read(saved).unwrap(), original);
        let after = std::fs::read(&path).unwrap();
        assert_eq!(
            write_at(&path, &exe, true, &fingerprint(Some(&after))).unwrap(),
            "No change; no backup needed"
        );
        assert!(write_at(&path, &exe, false, "stale").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), after);
        write_at(&path, &exe, false, &fingerprint(Some(&after))).unwrap();
        let cleaned = std::fs::read(&path).unwrap();
        assert_eq!(
            write_at(&path, &exe, false, &fingerprint(Some(&cleaned))).unwrap(),
            "No change; no backup needed"
        );
        assert_eq!(
            parse(Some(&cleaned)).unwrap()["hooks"]["Stop"][0]["hooks"][0]["command"],
            "foreign"
        );
        // Exact known test directory only; no user home or real config is used.
        std::fs::remove_dir_all(dir).unwrap();
    }
}
