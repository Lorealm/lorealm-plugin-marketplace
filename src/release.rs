use std::io::{Cursor, Read};
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use lorealm_plugin_manifest::{MANIFEST_FILE, Manifest};
use zip::ZipArchive;

use crate::entry::Pinned;
use crate::fetch::sha256;

/// How a WebAssembly component starts: the magic, then the component layer.
const COMPONENT: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];

/// A release that passed every check.
pub struct Release {
    pub manifest: Manifest,
    pub readme: Option<String>,
}

/// Checks that `bytes` is the zip `pinned` lists, and a plugin a server
/// would install as `handle`, without building anything.
pub fn inspect(handle: &str, pinned: &Pinned, bytes: &[u8]) -> Result<Release> {
    let hash = sha256(bytes);
    if hash != pinned.sha256 {
        bail!("its sha256 is {hash}, not the {} listed", pinned.sha256);
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("it isn't a zip")?;
    let manifest = manifest_in(&mut zip)?;

    let plugin = &manifest.plugin;
    if plugin.handle != handle {
        bail!(
            "its handle is `{}`, so it belongs in plugins/{}.toml",
            plugin.handle,
            plugin.handle
        );
    }
    if plugin.version != pinned.version {
        bail!(
            "its manifest is at version {}, not {}",
            plugin.version,
            pinned.version
        );
    }

    let backend = read(&mut zip, &manifest.structure.backend)?.with_context(|| {
        format!(
            "it has no backend at `{}`; a release holds the built plugin",
            manifest.structure.backend.display()
        )
    })?;
    if !backend.starts_with(&COMPONENT) {
        bail!("its backend isn't a WebAssembly component");
    }
    for slot in &manifest.frontend.slots {
        let entry = manifest.frontend.path.join(&slot.entry);
        if read(&mut zip, &entry)?.is_none() {
            bail!(
                "its `{}` slot's entry `{}` isn't in the zip",
                slot.slot,
                entry.display()
            );
        }
    }
    let readme =
        read(&mut zip, &manifest.structure.readme)?.and_then(|bytes| String::from_utf8(bytes).ok());

    Ok(Release { manifest, readme })
}

pub fn manifest_of(bytes: &[u8]) -> Result<Manifest> {
    manifest_in(&mut ZipArchive::new(Cursor::new(bytes)).context("it isn't a zip")?)
}

fn manifest_in(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Manifest> {
    let folder = tempfile::tempdir()?;
    let text = read(zip, Path::new(MANIFEST_FILE))?
        .with_context(|| format!("it has no {MANIFEST_FILE} at its top"))?;
    std::fs::write(folder.path().join(MANIFEST_FILE), text)?;
    Ok(Manifest::from_dir(folder.path())?)
}

fn read(zip: &mut ZipArchive<Cursor<&[u8]>>, path: &Path) -> Result<Option<Vec<u8>>> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy()),
            Component::CurDir => {}
            _ => bail!("`{}` points outside the plugin", path.display()),
        }
    }
    let Ok(mut file) = zip.by_name(&parts.join("/")) else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(test)]
pub mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    pub fn manifest(handle: &str, version: &str, extra: &str) -> String {
        format!(
            r#"
            [plugin]
            name = "Hello"
            handle = "{handle}"
            description = "Says hello."
            author = "Jane"
            version = "{version}"
            api = "0.1.0"
            {extra}
            "#
        )
    }

    pub fn zipped(version: &str, files: &[(&str, &[u8])]) -> (Pinned, Vec<u8>) {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let pinned = Pinned {
            version: version.parse().unwrap(),
            url: format!("https://github.com/jane/hello/releases/download/v{version}/hello.zip"),
            sha256: sha256(&bytes),
        };
        (pinned, bytes)
    }

    pub fn hello(version: &str, extra: &str) -> (Pinned, Vec<u8>) {
        zipped(
            version,
            &[
                (
                    "manifest.toml",
                    manifest("hello", version, extra).as_bytes(),
                ),
                ("plugin.wasm", &COMPONENT),
                ("README.md", b"# Hello\n\n![shot](docs/shot.png)\n"),
            ],
        )
    }

    fn refusal(handle: &str, pinned: &Pinned, bytes: &[u8]) -> String {
        inspect(handle, pinned, bytes).err().unwrap().to_string()
    }

    #[test]
    fn a_built_release_passes() {
        let (pinned, bytes) = hello("0.1.0", "");

        let release = inspect("hello", &pinned, &bytes).unwrap();

        assert_eq!(release.manifest.plugin.name, "Hello");
        assert!(release.readme.unwrap().starts_with("# Hello"));
    }

    #[test]
    fn a_zip_that_isnt_the_one_listed_is_refused() {
        let (mut pinned, bytes) = hello("0.1.0", "");
        pinned.sha256 = "0".repeat(64);

        assert!(refusal("hello", &pinned, &bytes).contains("sha256"));
    }

    #[test]
    fn a_release_under_another_handle_or_version_is_refused() {
        let (pinned, bytes) = hello("0.1.0", "");
        assert!(refusal("other", &pinned, &bytes).contains("plugins/hello.toml"));

        let (mut pinned, bytes) = hello("0.1.0", "");
        pinned.version = "0.2.0".parse().unwrap();
        assert!(refusal("hello", &pinned, &bytes).contains("version 0.1.0"));
    }

    #[test]
    fn a_release_without_a_built_backend_is_refused() {
        let text = manifest("hello", "0.1.0", "");
        let (pinned, bytes) = zipped("0.1.0", &[("manifest.toml", text.as_bytes())]);
        assert!(refusal("hello", &pinned, &bytes).contains("no backend"));

        let (pinned, bytes) = zipped(
            "0.1.0",
            &[
                ("manifest.toml", text.as_bytes()),
                ("plugin.wasm", b"\0asm\x01\0\0\0"),
            ],
        );
        assert!(refusal("hello", &pinned, &bytes).contains("component"));
    }

    #[test]
    fn a_missing_slot_entry_and_an_unknown_permission_are_refused() {
        let slot =
            "[[frontend.slots]]\nslot = \"message\"\nentry = \"message.html\"\ntitle = \"Hello\"";
        let (pinned, bytes) = hello("0.1.0", slot);
        assert!(refusal("hello", &pinned, &bytes).contains("message.html"));

        let (pinned, bytes) = hello("0.1.0", "permissions = [\"chat:own\"]");
        assert!(refusal("hello", &pinned, &bytes).contains("chat:own"));
    }
}
