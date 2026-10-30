use crate::manifest::ResolvedFolder;
use eyre::{Result, WrapErr, bail, eyre};
use indicatif::ProgressStyle;
#[cfg(test)]
use std::{
    env,
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{fs, io, path::Path};
use tracing::{debug, info, info_span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

pub(crate) fn run(profile: &str, folders: &[ResolvedFolder]) -> Result<()> {
    let folder_count = folders.len();
    let progress = info_span!("apply", profile, folders = folder_count);
    progress.pb_set_length(folder_count as u64);
    progress.pb_set_style(
        &ProgressStyle::with_template("{spinner} [{pos}/{len}] {wide_msg}")
            .wrap_err("invalid progress bar template")?,
    );
    let _progress_guard = progress.enter();

    info!(profile, "applying profile");
    for folder in folders {
        progress.pb_set_message(&format!("linking {}", folder.target.display()));
        create_folder_link(&folder.source, &folder.target)?;
        debug!(
            source = %folder.source.display(),
            target = %folder.target.display(),
            "linked folder"
        );
        progress.pb_inc(1);
    }
    progress.pb_set_finish_message(&format!("linked {folder_count} folder(s)"));

    Ok(())
}

fn create_folder_link(source: &Path, target: &Path) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if is_directory_link(target, &metadata)
                .wrap_err_with(|| format!("failed to inspect target {}", target.display()))?
                && fs::canonicalize(target).ok().as_deref() == Some(source)
            {
                return Ok(());
            }
            bail!(
                "target already exists and is not a link to {}: {}",
                source.display(),
                target.display()
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .wrap_err_with(|| format!("failed to inspect target {}", target.display()));
        }
    }

    let parent = target
        .parent()
        .ok_or_else(|| eyre!("target has no parent: {}", target.display()))?;
    fs::create_dir_all(parent)
        .wrap_err_with(|| format!("failed to create parent directory {}", parent.display()))?;
    create_directory_link(source, target).wrap_err_with(|| {
        format!(
            "failed to link folder {} to {}",
            target.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
fn is_directory_link(path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    Ok(metadata.file_type().is_symlink() || junction::exists(path)?)
}

#[cfg(not(windows))]
fn is_directory_link(_path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    Ok(metadata.file_type().is_symlink())
}

#[cfg(windows)]
fn create_directory_link(source: &Path, target: &Path) -> Result<()> {
    junction::create(source, target)?;
    Ok(())
}

#[cfg(unix)]
fn create_directory_link(source: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn create_directory_link(_source: &Path, _target: &Path) -> Result<()> {
    bail!("folder links are supported only on Unix and Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("dotty-{}-{unique}", process::id()));
        fs::create_dir_all(&path).expect("create temporary directory");
        path
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn folder_links_are_idempotent_and_preserve_existing_targets() -> Result<()> {
        let root = temporary_directory();
        let source = root.join("source");
        let target = root.join("links/source");
        fs::create_dir_all(&source)?;

        create_folder_link(&source, &target)?;
        create_folder_link(&source, &target)?;
        assert_eq!(fs::canonicalize(&target)?, fs::canonicalize(&source)?);

        let existing_target = root.join("existing");
        fs::create_dir_all(&existing_target)?;
        fs::write(existing_target.join("keep.txt"), "preserve")?;
        assert!(create_folder_link(&source, &existing_target).is_err());
        assert_eq!(
            fs::read_to_string(existing_target.join("keep.txt"))?,
            "preserve"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
