//! Raw data files (audio, PAMGuard binaries, CPOD/FPOD files...): found
//! through nereus.data_file + storage_root, then copied or downloaded.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use reqwest::Url;
use tokio::io::AsyncWriteExt;
use tokio_postgres::Client;

use crate::error::{Error, Result};
use crate::export::{Progress, Selection};

#[derive(Clone, Debug)]
pub struct DataFile {
    pub deployment: String,
    pub base_uri: String,
    pub rel_path: String,
    pub size_bytes: Option<i64>,
}

impl DataFile {
    pub fn url(&self) -> Option<Url> {
        let mut base = self.base_uri.clone();
        if !base.ends_with('/') {
            base.push('/');
        }
        Url::parse(&base).ok()?.join(&self.rel_path).ok()
    }

    /// Can this app fetch it? Local/network paths and http(s) for now.
    pub fn supported(&self) -> bool {
        self.url().is_some_and(|u| matches!(u.scheme(), "file" | "http" | "https"))
    }
}

pub async fn list(c: &Client, sel: &Selection) -> Result<Vec<DataFile>> {
    let rows = c
        .query(
            "SELECT dep.deployment_id, r.base_uri, f.rel_path, f.size_bytes
             FROM nereus.data_file f
             JOIN nereus.storage_root r ON r.id = f.storage_root_id
             JOIN nereus.deployment dep ON dep.id = f.deployment_id
             WHERE f.deployment_id = ANY($1)
               AND ($2::float8 IS NULL OR f.t_end IS NULL OR f.t_end >= to_timestamp($2))
               AND ($3::float8 IS NULL OR f.t_start IS NULL OR f.t_start < to_timestamp($3))
             ORDER BY dep.deployment_id, f.t_start NULLS FIRST, f.rel_path",
            &[&sel.deployment_ids, &sel.t0, &sel.t1],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| DataFile { deployment: r.get(0), base_uri: r.get(1), rel_path: r.get(2), size_bytes: r.get(3) })
        .collect())
}

/// A name that is safe as a single file or folder name on every OS.
pub fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect::<String>()
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// `rel` under `dir`, keeping its folders but never escaping `dir`.
fn target(dir: &Path, deployment: &str, rel: &str) -> PathBuf {
    let mut p = dir.join(safe_name(deployment));
    for part in Path::new(&rel.replace('\\', "/")).components() {
        if let Component::Normal(s) = part {
            p.push(safe_name(&s.to_string_lossy()));
        }
    }
    p
}

/// Copy/download every file. Returns (files written, warnings).
pub async fn download(
    found: &[DataFile],
    dir: &Path,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<(usize, Vec<String>)> {
    let total_bytes: u64 = found.iter().filter_map(|f| f.size_bytes).map(|b| b as u64).sum();
    let mut done_bytes = 0u64;
    let mut written = 0;
    let mut warnings = Vec::new();
    let http = reqwest::Client::builder()
        .user_agent(concat!("Nereus/", env!("CARGO_PKG_VERSION")))
        .build()?;

    for (i, f) in found.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let report = |done: u64| Progress {
            stage: format!("Downloading files ({} of {})", i + 1, found.len()),
            done,
            total: total_bytes,
            detail: f.rel_path.clone(),
        };
        progress(report(done_bytes));
        let Some(url) = f.url() else {
            warnings.push(format!("{}: can't make a location from '{}'", f.rel_path, f.base_uri));
            continue;
        };
        let dest = target(dir, &f.deployment, &f.rel_path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        match url.scheme() {
            "file" => {
                let Ok(src) = url.to_file_path() else {
                    warnings.push(format!("{url}: not a usable file path"));
                    continue;
                };
                match tokio::fs::copy(&src, &dest).await {
                    Ok(n) => {
                        done_bytes += n;
                        written += 1;
                    }
                    Err(e) => warnings.push(format!("{}: {e}", src.display())),
                }
            }
            "http" | "https" => {
                let resp = match http.get(url.clone()).send().await.and_then(|r| r.error_for_status()) {
                    Ok(r) => r,
                    Err(e) => {
                        warnings.push(format!("{url}: {e}"));
                        continue;
                    }
                };
                let mut out = tokio::fs::File::create(&dest).await?;
                let mut body = resp.bytes_stream();
                while let Some(chunk) = body.next().await {
                    if cancel.load(Ordering::Relaxed) {
                        drop(out);
                        let _ = tokio::fs::remove_file(&dest).await;
                        return Err(Error::Cancelled);
                    }
                    let chunk = chunk?;
                    out.write_all(&chunk).await?;
                    done_bytes += chunk.len() as u64;
                    progress(report(done_bytes));
                }
                out.flush().await?;
                written += 1;
            }
            other => warnings.push(format!(
                "{}: {other}:// storage isn't supported by this version yet",
                f.rel_path
            )),
        }
    }
    Ok((written, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_stay_inside() {
        let t = target(Path::new("out"), "HAT/01", "../../etc/passwd");
        assert_eq!(t, Path::new("out").join("HAT_01").join("etc").join("passwd"));
        let t = target(Path::new("out"), "D1", "2021\\01\\a.wav");
        assert_eq!(t, Path::new("out").join("D1").join("2021").join("01").join("a.wav"));
    }

    #[test]
    fn urls() {
        let f = DataFile {
            deployment: "d".into(),
            base_uri: "https://data.example.org/pam".into(),
            rel_path: "HAT 01/a.wav".into(),
            size_bytes: None,
        };
        assert_eq!(f.url().unwrap().as_str(), "https://data.example.org/pam/HAT%2001/a.wav");
        let s3 = DataFile { base_uri: "s3://bucket/x/".into(), ..f };
        assert!(!s3.supported());
    }
}
