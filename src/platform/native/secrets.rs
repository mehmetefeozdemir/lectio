//! API keys on the desktop (ADR-021): a small JSON file next to the library,
//! never inside it, with owner-only permissions where the OS has them, and
//! written via a temporary file so a crash can't leave half a file.

use std::collections::BTreeMap;
use std::path::Path;

pub const FILE_NAME: &str = "secrets.json";

fn read_all(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn load(path: &Path, name: &str) -> Option<String> {
    read_all(path).remove(name)
}

pub fn save(path: &Path, name: &str, value: Option<&str>) -> Result<(), String> {
    let mut all = read_all(path);
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => all.insert(name.to_owned(), value.to_owned()),
        None => all.remove(name),
    };
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not save the key: {e}"))?;
    }
    let temporary = path.with_extension("json.part");
    std::fs::write(&temporary, text).map_err(|e| format!("Could not save the key: {e}"))?;
    owner_only(&temporary);
    std::fs::rename(&temporary, path).map_err(|e| format!("Could not save the key: {e}"))
}

#[cfg(unix)]
fn owner_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_saved_replaced_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("lectio-secrets-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        save(&path, "openai", Some("test-value-1")).unwrap();
        save(&path, "openai", Some(" test-value-2 ")).unwrap();
        assert_eq!(load(&path, "openai").as_deref(), Some("test-value-2"));
        save(&path, "openai", None).unwrap();
        assert_eq!(load(&path, "openai"), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
