//! Read-only queries behind the globe, the deployment panel and the timeline.
//!
//! Times cross to the UI as seconds since 1970 (UTC, f64): cheap to send and
//! what JavaScript dates want (x1000).

use std::collections::HashMap;

use serde::Serialize;

use crate::db::Db;
use crate::error::{Error, Result};

/// One deployment as a point (or track) on the globe.
#[derive(Clone, Debug, Serialize)]
pub struct DeploymentSummary {
    pub id: i64,
    pub deployment_id: String,
    pub project: String,
    pub site: Option<String>,
    pub region: Option<String>,
    pub platform: String,
    pub deployment_number: i32,
    pub instrument_type: Option<String>,
    pub instrument_id: Option<String>,
    /// -180..180 (Tethys stores 0..360).
    pub lon: Option<f64>,
    pub lat: Option<f64>,
    pub depth_m: Option<f64>,
    pub t_deploy: f64,
    pub t_recover: Option<f64>,
    pub has_tracks: bool,
    pub sample_rate_khz: Option<f64>,
    pub n_detection_sets: i64,
    /// On-effort detections, from the daily summary.
    pub n_detections: i64,
    pub species: Vec<i64>,
    pub n_files: i64,
    pub file_bytes: i64,
}

pub async fn deployments(db: &Db) -> Result<Vec<DeploymentSummary>> {
    let c = db.pool.get().await?;
    let rows = c
        .query(
            "WITH det AS (
                 SELECT deployment_id, sum(n_detections)::int8 AS n,
                        array_agg(DISTINCT species_tsn ORDER BY species_tsn) AS species
                 FROM nereus.summary_daily WHERE deployment_id IS NOT NULL GROUP BY 1),
             sets AS (
                 SELECT deployment_id, count(*) AS n FROM nereus.effort
                 WHERE deployment_id IS NOT NULL GROUP BY 1),
             files AS (
                 SELECT deployment_id, count(*) AS n, coalesce(sum(size_bytes), 0)::int8 AS bytes
                 FROM nereus.data_file GROUP BY 1),
             rate AS (
                 SELECT DISTINCT ON (deployment_id) deployment_id, sample_rate_khz
                 FROM nereus.channel_sampling ORDER BY deployment_id, channel_ord, ord),
             start AS (       -- first track point, for drifters/towed arrays without a position
                 SELECT DISTINCT ON (deployment_id) deployment_id, lon, lat
                 FROM nereus.track_point WHERE lon IS NOT NULL AND lat IS NOT NULL
                 ORDER BY deployment_id, track_ord, ord)
             SELECT d.id, d.deployment_id, p.name, s.name, d.region, d.platform,
                    d.deployment_number, i.type, i.instrument_id,
                    coalesce(d.deploy_lon, start.lon), coalesce(d.deploy_lat, start.lat),
                    coalesce(d.deploy_depth_instrument_m, -d.deploy_elevation_instrument_m),
                    extract(epoch FROM d.t_deploy)::float8,
                    extract(epoch FROM d.t_recover)::float8,
                    d.has_tracks, rate.sample_rate_khz,
                    coalesce(sets.n, 0), coalesce(det.n, 0), coalesce(det.species, '{}'),
                    coalesce(files.n, 0), coalesce(files.bytes, 0)
             FROM nereus.deployment d
             JOIN nereus.project p ON p.id = d.project_id
             LEFT JOIN nereus.site s ON s.id = d.site_id
             LEFT JOIN nereus.instrument i ON i.id = d.instrument_id
             LEFT JOIN det ON det.deployment_id = d.id
             LEFT JOIN sets ON sets.deployment_id = d.id
             LEFT JOIN files ON files.deployment_id = d.id
             LEFT JOIN rate ON rate.deployment_id = d.id
             LEFT JOIN start ON start.deployment_id = d.id
             ORDER BY d.deployment_id",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| DeploymentSummary {
            id: r.get(0),
            deployment_id: r.get(1),
            project: r.get(2),
            site: r.get(3),
            region: r.get(4),
            platform: r.get(5),
            deployment_number: r.get(6),
            instrument_type: r.get(7),
            instrument_id: r.get(8),
            lon: r.get::<_, Option<f64>>(9).map(wrap_lon),
            lat: r.get(10),
            depth_m: r.get(11),
            t_deploy: r.get(12),
            t_recover: r.get(13),
            has_tracks: r.get(14),
            sample_rate_khz: r.get(15),
            n_detection_sets: r.get(16),
            n_detections: r.get(17),
            species: r.get(18),
            n_files: r.get(19),
            file_bytes: r.get(20),
        })
        .collect())
}

fn wrap_lon(lon: f64) -> f64 {
    if lon > 180.0 {
        lon - 360.0
    } else {
        lon
    }
}

/// The path of a drifter, glider or towed array, thinned for drawing.
#[derive(Clone, Debug, Serialize)]
pub struct Track {
    pub deployment: i64,
    /// [lat, lon] pairs.
    pub points: Vec<[f64; 2]>,
}

/// Points kept per track: enough to draw it, not to analyse it.
const TRACK_POINTS: i64 = 400;

pub async fn tracks(db: &Db) -> Result<Vec<Track>> {
    let c = db.pool.get().await?;
    let rows = c
        .query(
            "SELECT deployment_id, track_ord, lat, lon FROM (
                 SELECT deployment_id, track_ord, ord, lat, lon,
                        row_number() OVER w AS i, count(*) OVER w AS n
                 FROM nereus.track_point WHERE lat IS NOT NULL AND lon IS NOT NULL
                 WINDOW w AS (PARTITION BY deployment_id, track_ord)) x
             WHERE i % greatest(1, n / $1) = 0 OR i = 1 OR i = n
             ORDER BY deployment_id, track_ord, ord",
            &[&TRACK_POINTS],
        )
        .await?;
    let mut out: Vec<Track> = Vec::new();
    let mut last: Option<(i64, i32)> = None;
    for r in &rows {
        let key: (i64, i32) = (r.get(0), r.get(1));
        if last != Some(key) {
            out.push(Track { deployment: key.0, points: Vec::new() });
            last = Some(key);
        }
        let (lat, lon): (f64, f64) = (r.get(2), r.get(3));
        out.last_mut().unwrap().points.push([lat, wrap_lon(lon)]);
    }
    Ok(out)
}

#[derive(Clone, Debug, Serialize)]
pub struct Kind {
    pub species_tsn: i64,
    pub call: Option<String>,
    pub granularity: String,
    pub bin_size_s: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetectionSetInfo {
    pub doc_id: String,
    pub method: Option<String>,
    pub software: Option<String>,
    pub version: Option<String>,
    pub user_id: Option<String>,
    pub t_start: f64,
    pub t_end: f64,
    pub n_detections: i64,
    pub kinds: Vec<Kind>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileGroup {
    pub format: String,
    pub role: String,
    pub n: i64,
    pub bytes: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeploymentDetail {
    pub id: i64,
    pub alias: Option<String>,
    pub cruise: Option<String>,
    pub site_aliases: Vec<String>,
    pub abstract_text: Option<String>,
    pub objectives: Option<String>,
    pub n_channels: i64,
    pub sample_rates_khz: Vec<f64>,
    pub duty_cycled: bool,
    pub audio_uri: Option<String>,
    pub detection_sets: Vec<DetectionSetInfo>,
    pub files: Vec<FileGroup>,
}

/// Text of a Description field, whether stored as a plain value or with attributes.
fn text_of(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Object(o) => text_of(o.get("#text")),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub async fn deployment_detail(db: &Db, id: i64) -> Result<DeploymentDetail> {
    let c = db.pool.get().await?;
    let r = c
        .query_opt(
            "SELECT alias, cruise, coalesce(site_aliases, '{}'), description, audio_uri,
                    (SELECT count(*) FROM nereus.channel WHERE deployment_id = d.id),
                    (SELECT coalesce(array_agg(DISTINCT sample_rate_khz ORDER BY sample_rate_khz), '{}')
                       FROM nereus.channel_sampling WHERE deployment_id = d.id),
                    EXISTS (SELECT 1 FROM nereus.channel_duty_cycle WHERE deployment_id = d.id)
             FROM nereus.deployment d WHERE id = $1",
            &[&id],
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("deployment {id}")))?;
    let description: Option<serde_json::Value> = r.get(3);

    let sets = c
        .query(
            "SELECT s.id, s.doc_id, s.algorithm_method, s.algorithm_software, s.algorithm_version,
                    s.user_id, extract(epoch FROM e.t_start)::float8, extract(epoch FROM e.t_end)::float8,
                    (SELECT count(*) FROM nereus.detection x WHERE x.set_id = s.id)
             FROM nereus.effort e JOIN nereus.detection_set s ON s.id = e.set_id
             WHERE e.deployment_id = $1
                OR (e.deployment_id IS NULL AND EXISTS (
                        SELECT 1 FROM nereus.ensemble_unit u
                        WHERE u.ensemble_id = e.ensemble_id AND u.deployment_id = $1))
             ORDER BY e.t_start, s.doc_id",
            &[&id],
        )
        .await?;
    let set_ids: Vec<i64> = sets.iter().map(|s| s.get(0)).collect();
    let mut kinds: HashMap<i64, Vec<Kind>> = HashMap::new();
    for k in c
        .query(
            "SELECT set_id, species_tsn, call, granularity, bin_size_s
             FROM nereus.effort_kind WHERE set_id = ANY($1) ORDER BY set_id, ord",
            &[&set_ids],
        )
        .await?
    {
        kinds.entry(k.get(0)).or_default().push(Kind {
            species_tsn: k.get(1),
            call: k.get(2),
            granularity: k.get(3),
            bin_size_s: k.get(4),
        });
    }

    let files = c
        .query(
            "SELECT format, role, count(*), coalesce(sum(size_bytes), 0)::int8
             FROM nereus.data_file WHERE deployment_id = $1 GROUP BY 1, 2 ORDER BY 2, 1",
            &[&id],
        )
        .await?;

    Ok(DeploymentDetail {
        id,
        alias: r.get(0),
        cruise: r.get(1),
        site_aliases: r.get(2),
        abstract_text: text_of(description.as_ref().and_then(|d| d.get("Abstract"))),
        objectives: text_of(description.as_ref().and_then(|d| d.get("Objectives"))),
        audio_uri: r.get(4),
        n_channels: r.get(5),
        sample_rates_khz: r.get(6),
        duty_cycled: r.get(7),
        detection_sets: sets
            .iter()
            .map(|s| DetectionSetInfo {
                doc_id: s.get(1),
                method: s.get(2),
                software: s.get(3),
                version: s.get(4),
                user_id: s.get(5),
                t_start: s.get(6),
                t_end: s.get(7),
                n_detections: s.get(8),
                kinds: kinds.remove(&s.get::<_, i64>(0)).unwrap_or_default(),
            })
            .collect(),
        files: files
            .iter()
            .map(|f| FileGroup { format: f.get(0), role: f.get(1), n: f.get(2), bytes: f.get(3) })
            .collect(),
    })
}

// ------------------------------------------------------------------ timeline

/// A row of the timeline: one species (and call type).
#[derive(Clone, Debug, Serialize)]
pub struct Lane {
    pub key: String,
    pub species_tsn: i64,
    pub call: String,
    pub n_detections: i64,
}

fn lane_key(tsn: i64, call: &str) -> String {
    format!("{tsn}|{call}")
}

#[derive(Clone, Debug, Serialize)]
pub struct Span {
    pub lane: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TimelineMeta {
    pub lanes: Vec<Lane>,
    /// Analysis effort per lane: where someone looked. Detections outside
    /// it can't be compared with anything; absence inside it means absence.
    pub effort: Vec<Span>,
    /// Recording periods of the deployments (lane is the deployment id).
    pub recording: Vec<Span>,
    pub t_min: f64,
    pub t_max: f64,
}

pub async fn timeline_meta(db: &Db, ids: &[i64]) -> Result<TimelineMeta> {
    let c = db.pool.get().await?;
    let effort = c
        .query(
            "SELECT k.species_tsn, coalesce(k.call, ''),
                    extract(epoch FROM e.t_start)::float8, extract(epoch FROM e.t_end)::float8
             FROM nereus.effort e JOIN nereus.effort_kind k ON k.set_id = e.set_id
             WHERE e.deployment_id = ANY($1)
                OR (e.deployment_id IS NULL AND EXISTS (
                        SELECT 1 FROM nereus.ensemble_unit u
                        WHERE u.ensemble_id = e.ensemble_id AND u.deployment_id = ANY($1)))
             ORDER BY 1, 2, 3",
            &[&ids],
        )
        .await?;
    let counts = c
        .query(
            "SELECT species_tsn, call, sum(n_detections)::int8,
                    extract(epoch FROM min(day)::timestamp AT TIME ZONE 'UTC')::float8,
                    extract(epoch FROM (max(day) + 1)::timestamp AT TIME ZONE 'UTC')::float8
             FROM nereus.summary_daily WHERE deployment_id = ANY($1) GROUP BY 1, 2",
            &[&ids],
        )
        .await?;
    let recording = c
        .query(
            "SELECT deployment_id, extract(epoch FROM t_deploy)::float8,
                    extract(epoch FROM coalesce(t_recover, t_deploy))::float8
             FROM nereus.deployment WHERE id = ANY($1) ORDER BY t_deploy",
            &[&ids],
        )
        .await?;

    let mut lanes: Vec<Lane> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut add = |tsn: i64, call: String, n: i64, lanes: &mut Vec<Lane>| {
        let key = lane_key(tsn, &call);
        match index.get(&key) {
            Some(&i) => lanes[i].n_detections += n,
            None => {
                index.insert(key.clone(), lanes.len());
                lanes.push(Lane { key, species_tsn: tsn, call, n_detections: n });
            }
        }
    };
    let (mut t_min, mut t_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in &counts {
        add(r.get(0), r.get(1), r.get(2), &mut lanes);
        t_min = t_min.min(r.get(3));
        t_max = t_max.max(r.get(4));
    }
    let effort: Vec<Span> = effort
        .iter()
        .map(|r| {
            let (tsn, call): (i64, String) = (r.get(0), r.get(1));
            add(tsn, call.clone(), 0, &mut lanes);
            let s = Span { lane: lane_key(tsn, &call), start: r.get(2), end: r.get(3) };
            t_min = t_min.min(s.start);
            t_max = t_max.max(s.end);
            s
        })
        .collect();
    let recording: Vec<Span> = recording
        .iter()
        .map(|r| {
            let s = Span { lane: r.get(0), start: r.get(1), end: r.get(2) };
            t_min = t_min.min(s.start);
            t_max = t_max.max(s.end);
            s
        })
        .collect();
    // Most detections first; lanes with only effort (nothing found) last.
    lanes.sort_by(|a, b| b.n_detections.cmp(&a.n_detections).then(a.key.cmp(&b.key)));
    if !t_min.is_finite() {
        (t_min, t_max) = (0.0, 0.0);
    }
    Ok(TimelineMeta { lanes, effort, recording, t_min, t_max })
}

/// Detections in a time window: counts per bin when there are many,
/// individual detections when few enough to draw one by one.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TimelineData {
    /// "bins" or "events".
    pub mode: &'static str,
    pub t0: f64,
    pub t1: f64,
    pub n_bins: u32,
    /// Lane keys used below (indexes into this list).
    pub keys: Vec<String>,
    // bins mode: one entry per non-empty (bin, lane)
    pub bin: Vec<u32>,
    pub bin_key: Vec<u32>,
    pub count: Vec<i64>,
    // events mode
    pub start: Vec<f64>,
    pub end: Vec<f64>,
    pub event_key: Vec<u32>,
}

/// Above this many detections in view, show counts per bin instead.
pub const MAX_EVENTS: i64 = 20_000;

pub async fn timeline_data(db: &Db, ids: &[i64], t0: f64, t1: f64, n_bins: u32) -> Result<TimelineData> {
    let n_bins = n_bins.clamp(10, 4000);
    let c = db.pool.get().await?;
    let mut out = TimelineData { t0, t1, n_bins, ..Default::default() };
    let mut keys: HashMap<String, u32> = HashMap::new();
    let mut key_of = |tsn: i64, call: &str, out: &mut TimelineData| -> u32 {
        let k = lane_key(tsn, call);
        *keys.entry(k.clone()).or_insert_with(|| {
            out.keys.push(k);
            (out.keys.len() - 1) as u32
        })
    };

    // Cheap bounded count: stops once past the limit.
    let n: i64 = c
        .query_one(
            "SELECT count(*) FROM (
                 SELECT 1 FROM nereus.detection
                 WHERE deployment_id = ANY($1)
                   AND t_start >= to_timestamp($2) AND t_start < to_timestamp($3)
                 LIMIT $4) x",
            &[&ids, &t0, &t1, &(MAX_EVENTS + 1)],
        )
        .await?
        .get(0);

    if n <= MAX_EVENTS {
        out.mode = "events";
        for r in c
            .query(
                "SELECT species_tsn, coalesce(calls[1], ''),
                        extract(epoch FROM t_start)::float8,
                        extract(epoch FROM coalesce(t_end, t_start))::float8
                 FROM nereus.detection
                 WHERE deployment_id = ANY($1)
                   AND t_start >= to_timestamp($2) AND t_start < to_timestamp($3)
                 ORDER BY t_start",
                &[&ids, &t0, &t1],
            )
            .await?
        {
            let k = key_of(r.get(0), r.get::<_, &str>(1), &mut out);
            out.event_key.push(k);
            out.start.push(r.get(2));
            out.end.push(r.get(3));
        }
        return Ok(out);
    }

    out.mode = "bins";
    let bin_s = (t1 - t0) / n_bins as f64;
    // Days or longer per bin: the daily summary answers it without touching detections.
    let sql = if bin_s >= 86_400.0 {
        "SELECT width_bucket(extract(epoch FROM day::timestamp AT TIME ZONE 'UTC')::float8, $2, $3, $4) - 1,
                species_tsn, call, sum(n_detections)::int8
         FROM nereus.summary_daily
         WHERE deployment_id = ANY($1)
           AND day >= (to_timestamp($2) AT TIME ZONE 'UTC')::date
           AND day < (to_timestamp($3) AT TIME ZONE 'UTC')::date + 1
         GROUP BY 1, 2, 3"
    } else {
        "SELECT width_bucket(extract(epoch FROM t_start)::float8, $2, $3, $4) - 1,
                species_tsn, coalesce(calls[1], ''), count(*)
         FROM nereus.detection
         WHERE deployment_id = ANY($1)
           AND t_start >= to_timestamp($2) AND t_start < to_timestamp($3)
         GROUP BY 1, 2, 3"
    };
    for r in c.query(sql, &[&ids, &t0, &t1, &(n_bins as i32)]).await? {
        let b: i32 = r.get(0);
        if b < 0 || b >= n_bins as i32 {
            continue;
        }
        let k = key_of(r.get(1), r.get::<_, &str>(2), &mut out);
        out.bin.push(b as u32);
        out.bin_key.push(k);
        out.count.push(r.get(3));
    }
    Ok(out)
}
