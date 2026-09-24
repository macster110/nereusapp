//! Export a selection of deployments: MATLAB (.mat), R (.RData), Tethys XML
//! (zip of documents) and the raw data files.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::{pin_mut, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use tokio_postgres::Client;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::files;
use crate::mat::{self, MatValue};
use crate::rdata::{self, RObject};
use crate::table::{num, Column, Table};
use crate::tethys;

#[derive(Clone, Debug, Deserialize)]
pub struct Selection {
    pub deployment_ids: Vec<i64>,
    /// Seconds since 1970; detections starting in [t0, t1).
    pub t0: Option<f64>,
    pub t1: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExportRequest {
    #[serde(flatten)]
    pub selection: Selection,
    pub matlab: bool,
    pub r: bool,
    pub tethys: bool,
    pub raw: bool,
    pub out_dir: PathBuf,
    /// Base file name, e.g. "nereus_export" -> nereus_export.mat, ...
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportPreview {
    pub n_deployments: usize,
    pub n_detections: i64,
    pub n_documents: i64,
    pub n_files: i64,
    pub file_bytes: i64,
    /// Files whose storage can't be downloaded from here yet (e.g. s3://).
    pub n_files_unsupported: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ExportResult {
    pub written: Vec<String>,
    pub n_detections: u64,
    pub warnings: Vec<String>,
}

const TIME_FILTER: &str = "($2::float8 IS NULL OR d.t_start >= to_timestamp($2))
                       AND ($3::float8 IS NULL OR d.t_start < to_timestamp($3))";

/// Detection sets for the selected deployments whose effort overlaps the time range.
const DOCUMENTS: &str = "SELECT DISTINCT s.doc_id FROM nereus.effort e
     JOIN nereus.detection_set s ON s.id = e.set_id
     WHERE (e.deployment_id = ANY($1)
            OR (e.deployment_id IS NULL AND EXISTS (
                    SELECT 1 FROM nereus.ensemble_unit u
                    WHERE u.ensemble_id = e.ensemble_id AND u.deployment_id = ANY($1))))
       AND ($2::float8 IS NULL OR e.t_end >= to_timestamp($2))
       AND ($3::float8 IS NULL OR e.t_start < to_timestamp($3))
     ORDER BY 1";

pub async fn preview(db: &Db, sel: &Selection) -> Result<ExportPreview> {
    let c = db.pool.get().await?;
    let p: [&(dyn ToSql + Sync); 3] = [&sel.deployment_ids, &sel.t0, &sel.t1];
    let n_detections: i64 = c
        .query_one(
            &format!("SELECT count(*) FROM nereus.detection d WHERE d.deployment_id = ANY($1) AND {TIME_FILTER}"),
            &p,
        )
        .await?
        .get(0);
    let n_documents: i64 = c
        .query_one(&format!("SELECT count(*) FROM ({DOCUMENTS}) x"), &p)
        .await?
        .get(0);
    let found = files::list(&c, sel).await?;
    Ok(ExportPreview {
        n_deployments: sel.deployment_ids.len(),
        n_detections,
        n_documents: n_documents + sel.deployment_ids.len() as i64,
        n_files: found.len() as i64,
        file_bytes: found.iter().filter_map(|f| f.size_bytes).sum(),
        n_files_unsupported: found.iter().filter(|f| !f.supported()).count() as i64,
    })
}

/// Rows `[start, end)` of each deployment in a table sorted by deployment id.
fn ranges(t: &Table) -> HashMap<String, (usize, usize)> {
    let mut out = HashMap::new();
    let Some(Column::Str(ids)) = t.columns.first() else { return out };
    let mut start = 0;
    for i in 1..=ids.len() {
        if i == ids.len() || ids[i] != ids[start] {
            out.insert(ids[start].clone().unwrap_or_default(), (start, i));
            start = i;
        }
    }
    out
}

async fn deployments_table(c: &Client, ids: &[i64]) -> Result<Table> {
    let rows = c
        .query(
            "SELECT d.deployment_id, p.name, s.name, d.region, d.platform, d.deployment_number,
                    i.type, i.instrument_id,
                    d.deploy_lat,
                    CASE WHEN d.deploy_lon > 180 THEN d.deploy_lon - 360 ELSE d.deploy_lon END,
                    coalesce(d.deploy_depth_instrument_m, -d.deploy_elevation_instrument_m),
                    extract(epoch FROM d.t_deploy)::float8, extract(epoch FROM d.t_recover)::float8,
                    (SELECT sample_rate_khz FROM nereus.channel_sampling c
                      WHERE c.deployment_id = d.id ORDER BY channel_ord, ord LIMIT 1),
                    (SELECT count(*) FROM nereus.channel c WHERE c.deployment_id = d.id)::float8
             FROM nereus.deployment d
             JOIN nereus.project p ON p.id = d.project_id
             LEFT JOIN nereus.site s ON s.id = d.site_id
             LEFT JOIN nereus.instrument i ON i.id = d.instrument_id
             WHERE d.id = ANY($1) ORDER BY d.deployment_id",
            &[&ids],
        )
        .await?;
    let s = |i: usize| Column::Str(rows.iter().map(|r| r.get(i)).collect());
    let n = |i: usize| Column::Num(rows.iter().map(|r| num(r.get(i))).collect());
    let mut t = Table::new();
    t.add("deployment_id", s(0))
        .add("project", s(1))
        .add("site", s(2))
        .add("region", s(3))
        .add("platform", s(4))
        .add("deployment_number", Column::Num(rows.iter().map(|r| r.get::<_, i32>(5) as f64).collect()))
        .add("instrument_type", s(6))
        .add("instrument_id", s(7))
        .add("latitude", n(8))
        .add("longitude", n(9))
        .add("depth_m", n(10))
        .add("deployed", Column::Time(rows.iter().map(|r| num(r.get(11))).collect()))
        .add("recovered", Column::Time(rows.iter().map(|r| num(r.get(12))).collect()))
        .add("sample_rate_khz", n(13))
        .add("n_channels", n(14));
    Ok(t)
}

async fn effort_table(c: &Client, sel: &Selection) -> Result<Table> {
    let rows = c
        .query(
            "SELECT dep.deployment_id, s.doc_id, s.algorithm_method, s.algorithm_software,
                    s.algorithm_version, s.user_id,
                    extract(epoch FROM e.t_start)::float8, extract(epoch FROM e.t_end)::float8,
                    k.species_tsn::float8, k.call, k.granularity, k.bin_size_s
             FROM nereus.effort e
             JOIN nereus.detection_set s ON s.id = e.set_id
             JOIN nereus.effort_kind k ON k.set_id = e.set_id
             JOIN nereus.deployment dep ON dep.id = e.deployment_id
                  OR (e.deployment_id IS NULL AND dep.id IN (
                        SELECT u.deployment_id FROM nereus.ensemble_unit u
                        WHERE u.ensemble_id = e.ensemble_id))
             WHERE dep.id = ANY($1)
               AND ($2::float8 IS NULL OR e.t_end >= to_timestamp($2))
               AND ($3::float8 IS NULL OR e.t_start < to_timestamp($3))
             ORDER BY dep.deployment_id, e.t_start, s.doc_id, k.ord",
            &[&sel.deployment_ids, &sel.t0, &sel.t1],
        )
        .await?;
    let s = |i: usize| Column::Str(rows.iter().map(|r| r.get(i)).collect());
    let n = |i: usize| Column::Num(rows.iter().map(|r| num(r.get(i))).collect());
    let mut t = Table::new();
    t.add("deployment_id", s(0))
        .add("doc_id", s(1))
        .add("method", s(2))
        .add("software", s(3))
        .add("version", s(4))
        .add("user_id", s(5))
        .add("effort_start", Column::Time(rows.iter().map(|r| r.get(6)).collect()))
        .add("effort_end", Column::Time(rows.iter().map(|r| r.get(7)).collect()))
        .add("species_tsn", n(8))
        .add("call", s(9))
        .add("granularity", s(10))
        .add("bin_size_s", n(11));
    Ok(t)
}

async fn detections_table(
    c: &Client,
    sel: &Selection,
    total: u64,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<Table> {
    let stmt = c
        .prepare(&format!(
            "SELECT dep.deployment_id, s.doc_id,
                    extract(epoch FROM d.t_start)::float8, extract(epoch FROM d.t_end)::float8,
                    d.species_tsn::float8, d.species_group, array_to_string(d.calls, ';'), d.subtype,
                    d.channel::float8, d.count::float8, d.event, d.on_effort,
                    d.score, d.confidence, d.received_level_db, d.snr_db,
                    d.min_freq_hz, d.max_freq_hz, d.peak_freq_hz, d.duration_s, d.comment
             FROM nereus.detection d
             JOIN nereus.detection_set s ON s.id = d.set_id
             JOIN nereus.deployment dep ON dep.id = d.deployment_id
             WHERE d.deployment_id = ANY($1) AND {TIME_FILTER}
             ORDER BY dep.deployment_id, d.t_start, d.set_id, d.ord"
        ))
        .await?;
    const STR: [usize; 7] = [0, 1, 5, 6, 7, 10, 20];
    const NUM: [usize; 12] = [4, 8, 9, 12, 13, 14, 15, 16, 17, 18, 19, 3];
    let mut strs: Vec<Vec<Option<String>>> = vec![Vec::new(); STR.len()];
    let mut nums: Vec<Vec<f64>> = vec![Vec::new(); NUM.len()];
    let mut start: Vec<f64> = Vec::new();
    let mut on_effort: Vec<Option<bool>> = Vec::new();

    let params: [&(dyn ToSql + Sync); 3] = [&sel.deployment_ids, &sel.t0, &sel.t1];
    let rows = c.query_raw(&stmt, params).await?;
    pin_mut!(rows);
    let mut n = 0u64;
    while let Some(r) = rows.try_next().await? {
        for (col, &i) in strs.iter_mut().zip(&STR) {
            col.push(r.get(i));
        }
        for (col, &i) in nums.iter_mut().zip(&NUM) {
            col.push(num(r.get(i)));
        }
        start.push(r.get(2));
        on_effort.push(r.get(11));
        n += 1;
        if n.is_multiple_of(50_000) {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            progress(Progress { stage: "Reading detections".into(), done: n, total, detail: String::new() });
        }
    }
    let mut s = strs.into_iter();
    let mut x = nums.into_iter();
    let mut next_s = || Column::Str(s.next().unwrap());
    let mut next_n = || Column::Num(x.next().unwrap());
    let mut t = Table::new();
    t.add("deployment_id", next_s());
    t.add("doc_id", next_s());
    let species_group = next_s();
    let call = next_s();
    let subtype = next_s();
    let event = next_s();
    let comment = next_s();
    let species_tsn = next_n();
    let channel = next_n();
    let count = next_n();
    let [score, confidence, rl, snr, fmin, fmax, fpeak, dur]: [Column; 8] = std::array::from_fn(|_| next_n());
    let end = match next_n() {
        Column::Num(v) => Column::Time(v),
        c => c,
    };
    t.add("start", Column::Time(start))
        .add("end", end)
        .add("species_tsn", species_tsn)
        .add("species_group", species_group)
        .add("call", call)
        .add("subtype", subtype)
        .add("channel", channel)
        .add("count", count)
        .add("event", event)
        .add("on_effort", Column::Bool(on_effort))
        .add("score", score)
        .add("confidence", confidence)
        .add("received_level_db", rl)
        .add("snr_db", snr)
        .add("min_freq_hz", fmin)
        .add("max_freq_hz", fmax)
        .add("peak_freq_hz", fpeak)
        .add("duration_s", dur)
        .add("comment", comment);
    Ok(t)
}

fn info(db: &Db, sel: &Selection) -> Vec<(String, String)> {
    let t = |x: Option<f64>| {
        x.and_then(|s| chrono::DateTime::from_timestamp(s as i64, 0))
            .map_or_else(String::new, |d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    };
    vec![
        ("source".into(), format!("{} (port {})", db.settings.label(), db.settings.port)),
        ("created".into(), chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        ("time_from".into(), t(sel.t0)),
        ("time_to".into(), t(sel.t1)),
        ("software".into(), format!("Nereus {}", env!("CARGO_PKG_VERSION"))),
        (
            "notes".into(),
            "Times are UTC. species_tsn is the ITIS Taxonomic Serial Number. Effort rows say what \
             was looked for and when: no detections within effort means absence."
                .into(),
        ),
    ]
}

fn write_matlab(path: &Path, deployments: &Table, effort: &Table, detections: &Table, info: &[(String, String)]) -> Result<()> {
    let eff = ranges(effort);
    let det = ranges(detections);
    let mut fields: Vec<String> = deployments.names.clone();
    fields.push("effort".into());
    fields.push("detections".into());
    let Some(Column::Str(ids)) = deployments.columns.first() else { unreachable!() };
    let items: Vec<Vec<MatValue>> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut row: Vec<MatValue> = deployments
                .columns
                .iter()
                .map(|c| match c {
                    Column::Num(v) => MatValue::scalar(v[i]),
                    Column::Time(v) => MatValue::scalar(mat::datenum(v[i])),
                    Column::Str(v) => MatValue::text(v[i].clone().unwrap_or_default()),
                    Column::Bool(v) => MatValue::scalar(if v[i].unwrap_or(false) { 1.0 } else { 0.0 }),
                })
                .collect();
            let id = id.clone().unwrap_or_default();
            let part = |t: &Table, r: &HashMap<String, (usize, usize)>| {
                let (a, b) = r.get(&id).copied().unwrap_or((0, 0));
                MatValue::columns(&t.slice(a, b))
            };
            row.push(part(effort, &eff));
            row.push(part(detections, &det));
            row
        })
        .collect();
    let info = MatValue::record(info.iter().map(|(k, v)| (k.as_str(), MatValue::text(v.clone()))).collect());
    let mut f = BufWriter::new(File::create(path)?);
    mat::write(&mut f, &[("deployments", MatValue::struct_array(fields, items)), ("info", info)])?;
    Ok(())
}

fn write_r(path: &Path, deployments: &Table, effort: &Table, detections: &Table, info: Vec<(String, String)>) -> Result<()> {
    let f = BufWriter::new(File::create(path)?);
    rdata::write(
        f,
        &[
            ("deployments", RObject::DataFrame(deployments)),
            ("effort", RObject::DataFrame(effort)),
            ("detections", RObject::DataFrame(detections)),
            ("nereus_info", RObject::Named(info)),
        ],
    )?;
    Ok(())
}

async fn write_tethys(
    c: &Client,
    sel: &Selection,
    path: &Path,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<()> {
    let deployments: Vec<String> = c
        .query("SELECT deployment_id FROM nereus.deployment WHERE id = ANY($1) ORDER BY 1", &[&sel.deployment_ids])
        .await?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let docs: Vec<String> = c
        .query(DOCUMENTS, &[&sel.deployment_ids, &sel.t0, &sel.t1])
        .await?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let total = (deployments.len() + docs.len()) as u64;
    let mut zip = zip::ZipWriter::new(BufWriter::new(File::create(path)?));
    let mut opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(3))
        .large_file(true);
    if let Ok(now) = zip::DateTime::try_from(chrono::Local::now().naive_local()) {
        opts = opts.last_modified_time(now);
    }
    let mut done = 0u64;
    for (folder, list) in [("Deployments", &deployments), ("Detections", &docs)] {
        for id in list {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            progress(Progress { stage: "Writing Tethys XML".into(), done, total, detail: id.clone() });
            zip.start_file(format!("{folder}/{}.xml", files::safe_name(id)), opts)?;
            if folder == "Deployments" {
                tethys::export_deployment(c, id, &mut zip).await?;
            } else {
                tethys::export_detections(c, id, &mut zip).await?;
            }
            done += 1;
        }
    }
    zip.finish()?;
    Ok(())
}

/// Run an export. `progress` is called as it goes; set `cancel` to stop it.
pub async fn run(
    db: &Db,
    req: &ExportRequest,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<ExportResult> {
    let sel = &req.selection;
    if sel.deployment_ids.is_empty() {
        return Err(Error::Other("Nothing selected to export.".into()));
    }
    std::fs::create_dir_all(&req.out_dir)?;
    let name = files::safe_name(req.name.trim());
    let name = if name.is_empty() { "nereus_export".to_string() } else { name };
    let c = db.pool.get().await?;
    let mut result = ExportResult::default();

    if req.matlab || req.r {
        progress(Progress { stage: "Reading deployments".into(), done: 0, total: 0, detail: String::new() });
        let total: i64 = c
            .query_one(
                &format!("SELECT count(*) FROM nereus.detection d WHERE d.deployment_id = ANY($1) AND {TIME_FILTER}"),
                &[&sel.deployment_ids, &sel.t0, &sel.t1],
            )
            .await?
            .get(0);
        let deployments = deployments_table(&c, &sel.deployment_ids).await?;
        let effort = effort_table(&c, sel).await?;
        let detections = detections_table(&c, sel, total as u64, progress, cancel).await?;
        result.n_detections = detections.n_rows() as u64;
        let info = info(db, sel);
        if req.matlab {
            progress(Progress { stage: "Writing MATLAB file".into(), done: 0, total: 0, detail: String::new() });
            let p = req.out_dir.join(format!("{name}.mat"));
            write_matlab(&p, &deployments, &effort, &detections, &info)?;
            result.written.push(p.display().to_string());
        }
        if req.r {
            progress(Progress { stage: "Writing R file".into(), done: 0, total: 0, detail: String::new() });
            let p = req.out_dir.join(format!("{name}.RData"));
            write_r(&p, &deployments, &effort, &detections, info)?;
            result.written.push(p.display().to_string());
        }
    }

    if req.tethys {
        let p = req.out_dir.join(format!("{name}_tethys.zip"));
        write_tethys(&c, sel, &p, progress, cancel).await?;
        result.written.push(p.display().to_string());
    }

    if req.raw {
        let dir = req.out_dir.join(format!("{name}_files"));
        let found = files::list(&c, sel).await?;
        drop(c);
        let (n, warnings) = files::download(&found, &dir, progress, cancel).await?;
        result.warnings.extend(warnings);
        if n > 0 {
            result.written.push(dir.display().to_string());
        }
    }
    progress(Progress { stage: "Done".into(), done: 1, total: 1, detail: String::new() });
    Ok(result)
}
