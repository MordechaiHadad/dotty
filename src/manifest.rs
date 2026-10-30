use crate::paths::expand_target;
use eyre::{Result, WrapErr, bail, eyre};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};
use tracing::debug;

const MANIFEST_FILE: &str = "dotty.yaml";

pub(crate) struct Manifest {
    profiles: HashMap<String, ResolvedProfile>,
}

pub(crate) struct ResolvedProfile {
    folders: Vec<ResolvedFolder>,
}

impl Manifest {
    pub(crate) fn load() -> Result<Self> {
        let manifest_path = env::current_dir()
            .wrap_err("failed to get current directory")?
            .join(MANIFEST_FILE);
        debug!(manifest = %manifest_path.display(), "reading manifest");
        let manifest_text = fs::read_to_string(&manifest_path)
            .wrap_err_with(|| format!("failed to read {}", manifest_path.display()))?;
        let raw: RawManifest = serde_saphyr::from_str(&manifest_text)
            .wrap_err_with(|| format!("failed to parse {}", manifest_path.display()))?;
        if raw.version != 1 {
            bail!("unsupported manifest version {}; expected 1", raw.version);
        }

        let root = manifest_path
            .parent()
            .ok_or_else(|| eyre!("manifest has no parent directory"))?
            .canonicalize()
            .wrap_err("failed to resolve manifest directory")?;
        let home =
            dirs::home_dir().ok_or_else(|| eyre!("could not determine the home directory"))?;
        Self::build(raw, root, home)
    }

    pub(crate) fn folders(&self, profile: &str) -> Result<&[ResolvedFolder]> {
        self.profiles
            .get(profile)
            .map(|resolved| resolved.folders.as_slice())
            .ok_or_else(|| eyre!("unknown profile '{profile}'"))
    }

    fn build(raw: RawManifest, root: PathBuf, home: PathBuf) -> Result<Self> {
        let mut profiles = HashMap::with_capacity(raw.profiles.len());
        for name in raw.profiles.keys() {
            let folders = resolve_profile(name, &raw.profiles, &root, &home)?;
            profiles.insert(name.clone(), ResolvedProfile { folders });
        }
        Ok(Self { profiles })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    version: u32,
    profiles: HashMap<String, Profile>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    #[serde(default)]
    extends: Vec<String>,
    #[serde(default)]
    folders: Vec<Folder>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Folder {
    source: PathBuf,
    target: PathBuf,
}

#[derive(Debug)]
pub(crate) struct ResolvedFolder {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
}

struct ProfileResolver<'a> {
    profiles: &'a HashMap<String, Profile>,
    manifest_root: &'a Path,
    home: &'a Path,
    visiting: Vec<String>,
    visited: HashSet<String>,
    targets: HashSet<PathBuf>,
    folders: Vec<ResolvedFolder>,
}

fn resolve_profile(
    name: &str,
    profiles: &HashMap<String, Profile>,
    manifest_root: &Path,
    home: &Path,
) -> Result<Vec<ResolvedFolder>> {
    let mut resolver = ProfileResolver {
        profiles,
        manifest_root,
        home,
        visiting: Vec::new(),
        visited: HashSet::new(),
        targets: HashSet::new(),
        folders: Vec::new(),
    };
    resolver.collect(name)?;
    Ok(resolver.folders)
}

impl ProfileResolver<'_> {
    fn collect(&mut self, name: &str) -> Result<()> {
        if self.visiting.iter().any(|entry| entry == name) {
            let mut cycle = self.visiting.clone();
            cycle.push(name.to_owned());
            bail!("profile inheritance cycle: {}", cycle.join(" -> "));
        }
        if self.visited.contains(name) {
            return Ok(());
        }

        let profile = self
            .profiles
            .get(name)
            .ok_or_else(|| eyre!("unknown profile '{name}'"))?
            .clone();
        self.visiting.push(name.to_owned());

        for parent in &profile.extends {
            self.collect(parent)?;
        }

        for folder in &profile.folders {
            if folder.source.is_absolute() {
                bail!(
                    "source path must be relative to the manifest: {}",
                    folder.source.display()
                );
            }

            let source_path = self.manifest_root.join(&folder.source);
            let source = source_path.canonicalize().wrap_err_with(|| {
                format!(
                    "failed to resolve source directory {}",
                    source_path.display()
                )
            })?;
            if !source.is_dir() {
                bail!("source is not a directory: {}", source.display());
            }

            let target = expand_target(&folder.target, self.home)?;
            if target.starts_with(&source) {
                bail!(
                    "target must not be the source directory or one of its descendants: {}",
                    target.display()
                );
            }
            if !self.targets.insert(target.clone()) {
                bail!("multiple folders use target {}", target.display());
            }

            self.folders.push(ResolvedFolder { source, target });
        }

        self.visiting.pop();
        self.visited.insert(name.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("dotty-{}-{unique}", process::id()));
        fs::create_dir_all(&path).expect("create temporary directory");
        path
    }

    fn loaded(yaml: &str, root: PathBuf, home: PathBuf) -> Result<Manifest> {
        let raw: RawManifest = serde_saphyr::from_str(yaml)?;
        Manifest::build(raw, root, home)
    }

    #[test]
    fn yaml_profiles_inherit_folders() -> Result<()> {
        let root = temporary_directory();
        let repository = root.join("repository");
        let home = root.join("home");
        fs::create_dir_all(repository.join("shell"))?;
        fs::create_dir_all(repository.join("tools"))?;
        fs::create_dir_all(&home)?;
        let expected_shell_source = repository.join("shell").canonicalize()?;
        let expected_tools_source = repository.join("tools").canonicalize()?;
        let expected_shell_target = home.join("shell");
        let expected_tools_target = home.join("tools");

        let yaml = r#"
version: 1
profiles:
  base:
    folders:
      - source: shell
        target: ~/shell
  work:
    extends: [base]
    folders:
      - source: tools
        target: ~/tools
"#;
        let manifest = loaded(yaml, repository, home)?;
        let folders = manifest.folders("work")?;

        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].source, expected_shell_source);
        assert_eq!(folders[0].target, expected_shell_target);
        assert_eq!(folders[1].source, expected_tools_source);
        assert_eq!(folders[1].target, expected_tools_target);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn profile_inheritance_cycles_are_rejected() {
        let error = loaded(
            "version: 1\nprofiles:\n  first:\n    extends: [second]\n  second:\n    extends: [first]\n",
            PathBuf::from("."),
            PathBuf::from("/tmp"),
        )
        .err()
        .expect("cycle should be rejected");

        assert!(error.to_string().contains("inheritance cycle"));
    }

    #[test]
    fn duplicate_targets_are_rejected() -> Result<()> {
        let root = temporary_directory();
        fs::create_dir_all(root.join("repository/one"))?;
        fs::create_dir_all(root.join("repository/two"))?;

        let error = loaded(
            "version: 1\nprofiles:\n  base:\n    folders:\n      - source: one\n        target: ~/same\n  child:\n    extends: [base]\n    folders:\n      - source: two\n        target: ~/same\n",
            root.join("repository"),
            root.join("home"),
        )
        .err()
        .expect("duplicate target should be rejected");

        assert!(error.to_string().contains("multiple folders use target"));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
