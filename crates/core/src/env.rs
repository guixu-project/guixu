use std::path::{Path, PathBuf};

use anyhow::Result;

const SETTINGS_FILE_NAMES: [&str; 2] = ["settings.env", "setting.env"];

/// Load `local/settings.env` into the current process without overriding
/// variables that are already present in the environment.
pub fn load_local_settings() -> Result<()> {
    let Some(path) = resolve_local_settings_path() else {
        return Ok(());
    };

    for (key, value) in load_settings_from_path(&path)? {
        if std::env::var_os(&key).is_none() {
            std::env::set_var(key, value);
        }
    }

    Ok(())
}

/// Read a single value from `local/settings.env` without mutating process env.
pub fn load_setting_env_value(key: &str) -> Option<String> {
    let path = resolve_local_settings_path()?;
    load_settings_from_path(&path)
        .ok()?
        .into_iter()
        .find_map(|(name, value)| (name == key).then_some(value))
}

fn resolve_local_settings_path() -> Option<PathBuf> {
    for base in [
        std::env::current_dir().ok(),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(path) = find_local_settings_path_from(&base) {
            return Some(path);
        }
    }

    None
}

fn find_local_settings_path_from(base: &Path) -> Option<PathBuf> {
    for ancestor in base.ancestors() {
        for file_name in SETTINGS_FILE_NAMES {
            let candidate = ancestor.join("local").join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn load_settings_from_path(path: &Path) -> Result<Vec<(String, String)>> {
    let contents = std::fs::read_to_string(path)?;

    Ok(contents.lines().filter_map(parse_env_line).collect())
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let (name, raw_value) = line.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let value = raw_value.trim().trim_matches('"').trim_matches('\'').trim();
    if value.is_empty() {
        return None;
    }

    Some((name.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{find_local_settings_path_from, load_settings_from_path, parse_env_line};

    use std::path::PathBuf;

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("guixu-env-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_env_line_accepts_plain_and_exported_values() {
        assert_eq!(
            parse_env_line("DEEPSEEK_API_KEY=secret"),
            Some(("DEEPSEEK_API_KEY".into(), "secret".into()))
        );
        assert_eq!(
            parse_env_line("export GUIXU_HUB_BASE_URL='https://guixu.org'"),
            Some(("GUIXU_HUB_BASE_URL".into(), "https://guixu.org".into()))
        );
        assert_eq!(parse_env_line("# comment"), None);
        assert_eq!(parse_env_line(""), None);
    }

    #[test]
    fn find_local_settings_path_walks_up_ancestor_tree() {
        let root = temp_test_dir("resolve");
        let nested = root.join("crates").join("core").join("src");
        let settings_dir = root.join("local");
        let settings_path = settings_dir.join("settings.env");

        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(&settings_path, "DEEPSEEK_API_KEY=secret\n").unwrap();

        let resolved = find_local_settings_path_from(&nested);
        assert_eq!(resolved.as_deref(), Some(settings_path.as_path()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_settings_from_path_skips_blank_and_empty_values() {
        let root = temp_test_dir("load");
        let settings_path = root.join("settings.env");

        std::fs::write(
            &settings_path,
            "\n# comment\nDEEPSEEK_API_KEY=secret\nEMPTY_VALUE=\nexport HF_TOKEN=abc123\n",
        )
        .unwrap();

        let entries = load_settings_from_path(&settings_path).unwrap();
        assert_eq!(
            entries,
            vec![
                ("DEEPSEEK_API_KEY".into(), "secret".into()),
                ("HF_TOKEN".into(), "abc123".into()),
            ]
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
