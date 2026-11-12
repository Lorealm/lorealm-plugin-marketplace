use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// The largest release it downloads. This mirrors the server's setting.
const MAX_ZIP: u64 = 64 * 1024 * 1024;

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Downloads releases, keeping each under its hash so it's downloaded once.
pub struct Fetcher {
    cache: PathBuf,
}

impl Fetcher {
    pub fn new(cache: PathBuf) -> Self {
        Self { cache }
    }

    pub fn zip(&self, url: &str, expected: Option<&str>) -> Result<Vec<u8>> {
        let kept = |hash: &str| self.cache.join(format!("{hash}.zip"));
        let expected = expected.filter(|hash| hash.bytes().all(|b| b.is_ascii_hexdigit()));
        if let Some(bytes) = expected.and_then(|hash| std::fs::read(kept(hash)).ok()) {
            return Ok(bytes);
        }
        let bytes = ureq::get(url)
            .call()
            .with_context(|| format!("couldn't download {url}"))?
            .body_mut()
            .with_config()
            .limit(MAX_ZIP)
            .read_to_vec()
            .with_context(|| format!("couldn't download {url}"))?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::write(kept(&sha256(&bytes)), &bytes)?;
        Ok(bytes)
    }
}
