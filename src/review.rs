use std::path::Path;

use anyhow::{Result, bail};

use crate::entry::{Entry, Pinned};
use crate::fetch::Fetcher;
use crate::release::{Release, inspect};

/// Checks each changed plugin file in `files` against how it was in
/// `previous`, and prints a Markdown report of the versions they add. Fails
/// if anything is wrong.
///
/// This ensures a bad actor can't silently swap out an existing release
/// with malicious code.
pub fn run(fetcher: &Fetcher, previous: Option<&Path>, files: &[impl AsRef<Path>]) -> Result<()> {
    let mut problems = 0;
    for file in files {
        let file = file.as_ref();
        let before = previous
            .zip(file.file_name())
            .map(|(folder, name)| folder.join(name))
            .filter(|path| path.is_file());
        match review(fetcher, file, before.as_deref()) {
            Ok(report) => println!("{report}"),
            Err(error) => {
                problems += 1;
                println!("## {}\n\n**Problem:** {error:#}\n", file.display());
            }
        }
    }
    if problems > 0 {
        bail!("{problems} of the changed plugin files can't be accepted as they are");
    }
    Ok(())
}

fn review(fetcher: &Fetcher, file: &Path, before: Option<&Path>) -> Result<String> {
    let (handle, entry) = Entry::read(file)?;
    let before = before.map(Entry::read).transpose()?.map(|(_, entry)| entry);
    let listed: &[Pinned] = before.as_ref().map_or(&[], |b| &b.versions);

    if let Some(gone) = listed.iter().find(|p| !entry.versions.contains(p)) {
        bail!(
            "it changes or removes version {}, which is already listed; list a new version instead",
            gone.version
        );
    }
    let fetch = |pinned: &Pinned| -> Result<Release> {
        let bytes = fetcher.zip(&pinned.url, Some(&pinned.sha256))?;
        inspect(&handle, pinned, &bytes)
    };

    let mut report = String::new();
    if let Some(before) = before.as_ref().filter(|b| b.repository != entry.repository) {
        report.push_str(&format!(
            "## `{handle}`\n\n**Its repository changes** from {} to {}.\n\n",
            before.repository, entry.repository
        ));
    }
    let added: Vec<&Pinned> = entry
        .newest_first()
        .into_iter()
        .filter(|p| !listed.contains(p))
        .collect();
    for pinned in &added {
        let release = fetch(pinned)
            .map_err(|error| error.context(format!("version {} was refused", pinned.version)))?;
        let earlier = entry
            .newest_first()
            .into_iter()
            .find(|p| p.version < pinned.version);
        let earlier = earlier.and_then(|p| Some((&p.version, fetch(p).ok()?)));
        report.push_str(&section(&handle, pinned, &release, earlier));
    }
    if report.is_empty() {
        report = format!("## `{handle}`\n\nNo version is added or changed.\n");
    }
    Ok(report)
}

fn section(
    handle: &str,
    pinned: &Pinned,
    release: &Release,
    earlier: Option<(&semver::Version, Release)>,
) -> String {
    let manifest = &release.manifest;
    let plugin = &manifest.plugin;
    let code = |items: &[String]| -> String {
        if items.is_empty() {
            return "none".into();
        }
        let items: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
        items.join(", ")
    };
    let slots: Vec<String> = manifest
        .frontend
        .slots
        .iter()
        .map(|slot| match slot.r#unsafe {
            true => format!("{} (**unsafe**)", slot.slot),
            false => slot.slot.clone(),
        })
        .collect();

    let mut out = format!(
        "## {} `{handle}` {}\n\n{}\n\n",
        plugin.name, plugin.version, plugin.description
    );
    out.push_str(&format!("- **Author:** {}\n", plugin.author));
    out.push_str(&format!("- **Plugin API:** {}\n", plugin.api));
    out.push_str(&format!(
        "- **Permissions:** {}\n",
        code(&plugin.permissions)
    ));
    out.push_str(&format!(
        "- **Network hosts:** {}\n",
        code(&manifest.network.hosts)
    ));
    out.push_str(&format!(
        "- **Frontend slots:** {}\n",
        if slots.is_empty() {
            "none".into()
        } else {
            slots.join(", ")
        }
    ));
    out.push_str(&format!("- **Release:** {}\n\n", pinned.url));

    if plugin.permissions.iter().any(|p| p == "ui:unsafe") {
        out.push_str("**It asks for `ui:unsafe`:** its frontend can run outside the sandbox, with the client's full access.\n\n");
    }
    match earlier {
        None => out.push_str("This is the first version listed.\n"),
        Some((version, earlier)) => {
            let old = &earlier.manifest;
            let gained = |new: &[String], old: &[String]| -> Vec<String> {
                new.iter().filter(|n| !old.contains(n)).cloned().collect()
            };
            let permissions = gained(&plugin.permissions, &old.plugin.permissions);
            let hosts = gained(&manifest.network.hosts, &old.network.hosts);
            if permissions.is_empty() && hosts.is_empty() {
                out.push_str(&format!(
                    "It can reach nothing version {version} couldn't.\n"
                ));
            } else {
                out.push_str(&format!("**Since {version}, it gains:**\n\n"));
                if !permissions.is_empty() {
                    out.push_str(&format!("- the permissions {}\n", code(&permissions)));
                }
                if !hosts.is_empty() {
                    out.push_str(&format!("- the network hosts {}\n", code(&hosts)));
                }
            }
        }
    }
    out
}
