use eyre::{Result, bail, eyre};
use std::path::{Component, Path, PathBuf};

pub(crate) fn expand_target(path: &Path, home: &Path) -> Result<PathBuf> {
    let path_text = path.to_string_lossy();
    let expanded = if path_text == "~" {
        home.to_path_buf()
    } else if let Some(remainder) = path_text
        .strip_prefix("~/")
        .or_else(|| path_text.strip_prefix("~\\"))
    {
        home.join(remainder)
    } else if let Some((name, remainder)) = path_text.split_once(':') {
        match preset_directory(name) {
            Some(directory) => {
                let remainder = remainder.trim_start_matches(['/', '\\']);
                if Path::new(remainder).is_absolute() {
                    bail!(
                        "target must be relative to the {name} preset: {}",
                        path.display()
                    );
                }
                directory?.join(remainder)
            }
            None => path.to_path_buf(),
        }
    } else {
        path.to_path_buf()
    };

    if !expanded.is_absolute() {
        bail!(
            "target must be absolute, start with '~', or use a preset like 'config:/...': {}",
            path.display()
        );
    }

    Ok(normalize_absolute(&expanded))
}

fn preset_directory(name: &str) -> Option<Result<PathBuf>> {
    let directory = match name {
        "config" => dirs::config_dir(),
        "config-local" => dirs::config_local_dir(),
        "data" => dirs::data_dir(),
        "data-local" => dirs::data_local_dir(),
        "cache" => dirs::cache_dir(),
        "state" => dirs::state_dir(),
        _ => return None,
    };
    Some(directory.ok_or_else(|| eyre!("{name} directory is not available on this platform")))
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn preset_targets_expand_to_platform_directories() -> Result<()> {
        let home = env::temp_dir();
        let config_root = dirs::config_dir().expect("config directory is available");

        assert_eq!(
            expand_target(Path::new("config:"), &home)?,
            normalize_absolute(&config_root)
        );
        assert_eq!(
            expand_target(Path::new("config:/nvim"), &home)?,
            normalize_absolute(&config_root.join("nvim"))
        );
        assert_eq!(
            expand_target(Path::new("~/.config/nvim"), &home)?,
            home.join(".config/nvim")
        );

        Ok(())
    }

    #[test]
    fn unknown_presets_are_rejected_as_relative_paths() {
        let error = expand_target(Path::new("nope:/target"), &env::temp_dir())
            .expect_err("unknown preset should not be accepted");
        assert!(error.to_string().contains("must be absolute"));
    }
}
