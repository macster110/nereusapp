//! Species names for ITIS Taxonomic Serial Numbers, looked up from the ITIS
//! web service and cached on disk so each is only fetched once.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpeciesName {
    pub scientific: Option<String>,
    pub common: Option<String>,
}

const ITIS: &str = "https://www.itis.gov/ITISWebService/jsonservice";

async fn fetch(http: &reqwest::Client, tsn: i64) -> Option<SpeciesName> {
    let get = |method: &str| {
        let url = format!("{ITIS}/{method}?tsn={tsn}");
        // ITIS sometimes sends Latin-1 (e.g. French common names), which a
        // strict UTF-8 JSON parse rejects; the English names survive this.
        async move {
            let bytes = http.get(url).send().await.ok()?.bytes().await.ok()?;
            serde_json::from_str::<Value>(&String::from_utf8_lossy(&bytes)).ok()
        }
    };
    let sci = get("getScientificNameFromTSN").await?;
    let scientific = sci.get("combinedName").and_then(Value::as_str).map(str::to_string);
    let common = get("getCommonNamesFromTSN").await.and_then(|v| {
        let names = v.get("commonNames")?.as_array()?;
        let english = |n: &&Value| n.get("language").and_then(Value::as_str) == Some("English");
        let pick = names.iter().find(english).or_else(|| names.iter().find(|n| !n.is_null()))?;
        pick.get("commonName").and_then(Value::as_str).map(str::to_string)
    });
    // ITIS answers unknown numbers with empty fields: don't cache those.
    scientific.as_ref().filter(|s| !s.is_empty())?;
    Some(SpeciesName { scientific, common })
}

/// Names for the given TSNs. Unknown or unreachable ones are left out.
/// Negative numbers are Tethys local codes (not ITIS) and are never looked up.
pub async fn names(tsns: &[i64], cache_file: &Path) -> HashMap<i64, SpeciesName> {
    let mut cache: HashMap<i64, SpeciesName> = std::fs::read(cache_file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let missing: Vec<i64> = tsns.iter().copied().filter(|t| *t > 0 && !cache.contains_key(t)).collect();
    if !missing.is_empty() {
        if let Ok(http) = reqwest::Client::builder().timeout(Duration::from_secs(8)).build() {
            let found: Vec<(i64, Option<SpeciesName>)> = futures_util::stream::iter(missing)
                .map(|t| {
                    let http = &http;
                    async move { (t, fetch(http, t).await) }
                })
                .buffer_unordered(6)
                .collect()
                .await;
            let mut changed = false;
            for (t, n) in found {
                if let Some(n) = n {
                    cache.insert(t, n);
                    changed = true;
                }
            }
            if changed {
                if let Some(dir) = cache_file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Ok(b) = serde_json::to_vec_pretty(&cache) {
                    let _ = std::fs::write(cache_file, b);
                }
            }
        }
    }
    tsns.iter().filter_map(|t| cache.get(t).map(|n| (*t, n.clone()))).collect()
}
