use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::entry::{Entry, Pinned, files};
use crate::fetch::Fetcher;
use crate::readme;
use crate::release::{Release, inspect};

/// The index's format.
const SCHEMA: u32 = 1;

/// Writes `index.json` and each plugin's README into `out`.
pub fn build(fetcher: &Fetcher, out: &Path) -> Result<()> {
    let readmes = out.join("readme");
    std::fs::create_dir_all(&readmes)?;
    let mut plugins = Vec::new();

    for file in files()? {
        let (handle, entry) = Entry::read(&file)?;
        let mut versions = Vec::new();
        let mut readme = None;
        for pinned in entry.newest_first() {
            let release = fetcher
                .zip(&pinned.url, Some(&pinned.sha256))
                .and_then(|bytes| inspect(&handle, pinned, &bytes));
            match release {
                Ok(release) => {
                    if versions.is_empty()
                        && let Some(text) = &release.readme
                    {
                        let text =
                            readme::ensure_absolute_paths(text, &entry.repository, pinned.tag());
                        std::fs::write(readmes.join(format!("{handle}.md")), text)?;
                        readme = Some(format!("readme/{handle}.md"));
                    }
                    versions.push(version(pinned, &release));
                }
                Err(error) => eprintln!("Left out {handle} {}: {error:#}", pinned.version),
            }
        }
        if !versions.is_empty() {
            plugins.push(json!({
                "handle": handle,
                "repository": entry.repository,
                "readme": readme,
                "versions": versions,
            }));
        }
    }

    let count = plugins.len();
    let index = json!({ "schema": SCHEMA, "plugins": plugins });
    std::fs::write(out.join("index.json"), serde_json::to_string(&index)?)?;
    println!(
        "Wrote {count} plugins to {}",
        out.join("index.json").display()
    );
    Ok(())
}

fn version(pinned: &Pinned, release: &Release) -> Value {
    let manifest = &release.manifest;
    let plugin = &manifest.plugin;
    let slots: Vec<Value> = manifest
        .frontend
        .slots
        .iter()
        .map(|slot| json!({ "slot": slot.slot, "title": slot.title, "unsafe": slot.r#unsafe }))
        .collect();
    json!({
        "version": plugin.version,
        "api": plugin.api,
        "url": pinned.url,
        "sha256": pinned.sha256,
        "name": plugin.name,
        "description": plugin.description,
        "author": plugin.author,
        "tags": plugin.tags,
        "permissions": plugin.permissions,
        "hosts": manifest.network.hosts,
        "slots": slots,
        "requires": manifest.compatibility.requires,
        "recommends": manifest.compatibility.recommends,
        "conflicts": manifest.compatibility.conflicts,
    })
}
