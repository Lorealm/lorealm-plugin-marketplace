use std::path::Path;

use anyhow::{Context, Result};

use crate::entry::Entry;

/// What the plugin files in `current` take back from those in `previous`,
/// as Markdown for a pull request comment. Empty when the change only adds
/// plugins and versions.
pub fn findings(previous: &Path, current: &Path) -> Result<String> {
    let mut names: Vec<_> = std::fs::read_dir(previous)
        .with_context(|| format!("couldn't read {}", previous.display()))?
        .filter_map(|entry| Some(entry.ok()?.file_name()))
        .filter(|name| Path::new(name).extension().is_some_and(|e| e == "toml"))
        .collect();
    names.sort();

    let mut found = Vec::new();
    for name in names {
        let handle = Path::new(&name).with_extension("");
        let handle = handle.display();
        let parse = |folder: &Path| -> Result<Entry> {
            Ok(toml::from_str(&std::fs::read_to_string(
                folder.join(&name),
            )?)?)
        };
        // A file that was already unreadable has nothing to take back.
        let Ok(before) = parse(previous) else {
            continue;
        };
        if !current.join(&name).is_file() {
            found.push(format!("removes the plugin `{handle}`"));
            continue;
        }
        let Ok(after) = parse(current) else {
            found.push(format!("leaves `{handle}`'s file unreadable"));
            continue;
        };
        if after.repository != before.repository {
            found.push(format!(
                "moves `{handle}` from {} to {}",
                before.repository, after.repository
            ));
        }
        for listed in &before.versions {
            let version = &listed.version;
            match after.versions.iter().find(|p| p.version == *version) {
                None => found.push(format!("removes `{handle}` {version}")),
                Some(now) if now.sha256 != listed.sha256 => found.push(format!(
                    "changes the sha256 of `{handle}` {version}, so a different zip would be installed as that version"
                )),
                Some(now) if now.url != listed.url => found.push(format!(
                    "changes where `{handle}` {version} is downloaded from"
                )),
                Some(_) => {}
            }
        }
    }
    if found.is_empty() {
        return Ok(String::new());
    }
    let list: String = found.iter().map(|f| format!("- It {f}.\n")).collect();
    Ok(format!(
        "### This pull request changes what's already listed\n\nA pull request normally only adds a plugin or a version. This one does more:\n\n{list}\nVersions that are already listed were reviewed as they are, and servers install them by their hash. Merge this only if the change is meant.\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY: &str = "https://github.com/jane/hello";

    fn file(repository: &str, versions: &[(&str, &str)]) -> String {
        let mut text = format!("repository = \"{repository}\"\n");
        for (version, hash) in versions {
            text.push_str(&format!(
                "[[versions]]\nversion = \"{version}\"\nurl = \"{repository}/releases/download/v{version}/hello.zip\"\nsha256 = \"{}\"\n",
                hash.repeat(64)
            ));
        }
        text
    }

    fn found(before: &str, after: Option<&str>) -> String {
        let folder = tempfile::tempdir().unwrap();
        let (previous, current) = (folder.path().join("a"), folder.path().join("b"));
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(previous.join("hello.toml"), before).unwrap();
        if let Some(after) = after {
            std::fs::write(current.join("hello.toml"), after).unwrap();
        }
        // A plugin listed for the first time is never a finding.
        std::fs::write(
            current.join("new.toml"),
            file(REPOSITORY, &[("1.0.0", "c")]),
        )
        .unwrap();
        findings(&previous, &current).unwrap()
    }

    #[test]
    fn adding_a_version_is_fine() {
        let before = file(REPOSITORY, &[("0.1.0", "a")]);
        let after = file(REPOSITORY, &[("0.2.0", "b"), ("0.1.0", "a")]);

        assert_eq!(found(&before, Some(&after)), "");
    }

    #[test]
    fn a_swapped_hash_under_a_new_version_is_found() {
        let before = file(REPOSITORY, &[("0.1.0", "a")]);
        let after = file(REPOSITORY, &[("0.2.0", "b"), ("0.1.0", "f")]);

        let found = found(&before, Some(&after));

        assert!(
            found.contains("changes the sha256 of `hello` 0.1.0"),
            "{found}"
        );
    }

    #[test]
    fn removals_and_moves_are_found() {
        let before = file(REPOSITORY, &[("0.2.0", "b"), ("0.1.0", "a")]);
        let fewer = file(REPOSITORY, &[("0.2.0", "b")]);
        let moved = file("https://github.com/mallory/hello", &[("0.2.0", "b")]);

        assert!(found(&before, Some(&fewer)).contains("removes `hello` 0.1.0"));
        assert!(found(&before, Some(&moved)).contains("moves `hello` from"));
        assert!(found(&before, None).contains("removes the plugin `hello`"));
        assert!(found(&before, Some("not toml [")).contains("unreadable"));
    }
}
