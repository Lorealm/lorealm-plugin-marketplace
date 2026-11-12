use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::fetch::{Fetcher, sha256};
use crate::release;

/// Where the plugin files are: one per plugin, named after its handle.
pub const PLUGINS: &str = "plugins";

/// A plugin's file: where it's developed, and the releases reviewed so far.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// `https://github.com/owner/name`. Every release is one of its own.
    pub repository: String,
    #[serde(default)]
    pub versions: Vec<Pinned>,
}

/// One reviewed release. The hash is what makes the review hold: a server
/// installs the zip only if it's still the one that was looked at.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pinned {
    pub version: Version,
    pub url: String,
    pub sha256: String,
}

impl Pinned {
    /// The release's git tag, which its link names.
    pub fn tag(&self) -> &str {
        let after = self.url.split("/releases/download/").nth(1);
        after
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default()
    }
}

impl Entry {
    /// Reads the plugin file at `path`, with the handle its name gives the
    /// plugin.
    pub fn read(path: &Path) -> Result<(String, Self)> {
        let handle = path
            .file_stem()
            .filter(|_| path.extension().is_some_and(|e| e == "toml"))
            .map(|stem| stem.to_string_lossy().into_owned())
            .with_context(|| format!("{} isn't a `<handle>.toml` file", path.display()))?;
        if !is_identifier(&handle) {
            bail!("`{handle}` isn't a handle: letters, digits and `_`, not starting with a digit");
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("couldn't read {}", path.display()))?;
        let entry: Self = toml::from_str(&text)?;
        entry.validate()?;
        Ok((handle, entry))
    }

    fn validate(&self) -> Result<()> {
        let repository = &self.repository;
        let path = repository.strip_prefix("https://github.com/").unwrap_or("");
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) || path.ends_with(".git") {
            bail!("`{repository}` isn't a repository like `https://github.com/owner/name`");
        }
        if self.versions.is_empty() {
            bail!("it lists no versions");
        }
        for (i, pinned) in self.versions.iter().enumerate() {
            let version = &pinned.version;
            if self.versions[..i].iter().any(|p| p.version == *version) {
                bail!("it lists version {version} twice");
            }
            if !pinned
                .url
                .starts_with(&format!("{repository}/releases/download/"))
            {
                bail!("version {version} isn't a release of {repository}");
            }
            let hash = &pinned.sha256;
            if hash.len() != 64 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
                bail!("version {version}'s sha256 isn't 64 lowercase hex digits");
            }
        }
        Ok(())
    }

    /// Newest first.
    pub fn newest_first(&self) -> Vec<&Pinned> {
        let mut versions: Vec<&Pinned> = self.versions.iter().collect();
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        versions
    }
}

/// Every plugin file, in order.
pub fn files() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(PLUGINS)
        .with_context(|| format!("there's no `{PLUGINS}` folder here"))?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    Ok(files)
}

/// Lists the release whose zip is at `url` in its plugin's file, creating
/// the file for a plugin's first.
pub fn add(fetcher: &Fetcher, url: &str) -> Result<()> {
    let repository = url
        .split_once("/releases/download/")
        .map(|(repository, _)| repository.to_owned())
        .context("that isn't a link to a zip attached to a GitHub release")?;
    let bytes = fetcher.zip(url, None)?;
    let manifest = release::manifest_of(&bytes)?;
    let handle = manifest.plugin.handle.clone();
    let pinned = Pinned {
        version: manifest.plugin.version.clone(),
        url: url.to_owned(),
        sha256: sha256(&bytes),
    };

    let path = Path::new(PLUGINS).join(format!("{handle}.toml"));
    let mut entry = if path.is_file() {
        Entry::read(&path)?.1
    } else {
        Entry {
            repository: repository.clone(),
            versions: Vec::new(),
        }
    };
    if entry.repository != repository {
        bail!(
            "`{handle}` is listed as a plugin of {}, and this release is from {repository}",
            entry.repository
        );
    }
    entry.versions.insert(0, pinned.clone());
    entry.versions.sort_by(|a, b| b.version.cmp(&a.version));
    entry.validate()?;
    release::inspect(&handle, &pinned, &bytes)?;

    std::fs::write(&path, toml::to_string(&entry)?)?;
    println!("Listed {handle} {} in {}", pinned.version, path.display());
    Ok(())
}

/// The server's rule for a handle.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> Result<Entry> {
        let entry: Entry = toml::from_str(text)?;
        entry.validate()?;
        Ok(entry)
    }

    fn listed(repository: &str, url: &str, sha256: &str) -> String {
        format!(
            "repository = \"{repository}\"\n[[versions]]\nversion = \"0.1.0\"\nurl = \"{url}\"\nsha256 = \"{sha256}\"\n"
        )
    }

    const REPOSITORY: &str = "https://github.com/jane/hello";
    const URL: &str = "https://github.com/jane/hello/releases/download/v0.1.0/hello-0.1.0.zip";

    #[test]
    fn a_release_of_the_repository_is_accepted() {
        let entry = entry(&listed(REPOSITORY, URL, &"a".repeat(64))).unwrap();

        assert_eq!(entry.versions[0].tag(), "v0.1.0");
    }

    #[test]
    fn a_release_from_somewhere_else_is_refused() {
        let elsewhere = "https://example.com/hello-0.1.0.zip";
        let other = "https://github.com/mallory/hello/releases/download/v0.1.0/hello.zip";

        assert!(entry(&listed(REPOSITORY, elsewhere, &"a".repeat(64))).is_err());
        assert!(entry(&listed(REPOSITORY, other, &"a".repeat(64))).is_err());
        assert!(entry(&listed("https://github.com/jane", URL, &"a".repeat(64))).is_err());
    }

    #[test]
    fn a_hash_that_isnt_one_and_a_repeated_version_are_refused() {
        assert!(entry(&listed(REPOSITORY, URL, "abc")).is_err());

        let twice = listed(REPOSITORY, URL, &"a".repeat(64))
            + &format!(
                "[[versions]]\nversion = \"0.1.0\"\nurl = \"{URL}\"\nsha256 = \"{}\"\n",
                "b".repeat(64)
            );
        assert!(entry(&twice).unwrap_err().to_string().contains("twice"));
    }
}
