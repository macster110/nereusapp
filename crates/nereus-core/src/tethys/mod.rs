//! Tethys/ASA XML documents rebuilt from the database: a port of the Python
//! exporters (nereus/export.py and nereus/deployment.py), element for element.

pub mod fmt;

use std::collections::HashMap;
use std::io::Write;

use chrono::{DateTime, Utc};
use futures_util::{pin_mut, TryStreamExt};
use serde_json::Value;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Row};

use crate::error::{Error, Result};
use fmt::*;

fn time(t: DateTime<Utc>) -> String {
    format_time(&t)
}

fn exact_of<'a>(exact: &'a Option<Value>, key: &str) -> Option<&'a str> {
    exact.as_ref()?.get(key)?.as_str()
}

fn species(tsn: i64, group: Option<String>, depth: usize) -> String {
    el("SpeciesId", Some(&tsn.to_string()), depth, &[("Group", group)])
}

// ------------------------------------------------------------- Detections

/// Float arrays are formatted by PostgreSQL (shortest exact form), as the
/// Python exporter does: much faster for long whistle contours.
const DETECTION_SELECT: &str = "SELECT input_file, t_start, t_end, count, event, unit_id, channel,
        species_tsn, species_group, calls, has_parameters, subtype, score, confidence, qa,
        received_level_db, array_to_string(freq_measurements_db, ' ') AS freq_measurements_db,
        snr_db, min_freq_hz, max_freq_hz, peak_freq_hz,
        array_to_string(peaks_hz, ' ') AS peaks_hz, duration_s,
        array_to_string(sideband_hz, ' ') AS sideband_hz, has_tonal,
        array_to_string(tonal_offset_s, ' ') AS tonal_offset_s,
        array_to_string(tonal_hz, ' ') AS tonal_hz, array_to_string(tonal_db, ' ') AS tonal_db,
        event_ref, user_defined, user_defined_xml, image, audio, comment
     FROM nereus.detection WHERE set_id = $1 AND on_effort = $2 ORDER BY ord";

fn detection(r: &Row, ns: Option<&str>) -> String {
    let d = 3;
    let s = |c: &str| r.get::<_, Option<String>>(c);
    let n = |c: &str| r.get::<_, Option<f64>>(c);
    let i = |c: &str| r.get::<_, Option<i64>>(c);
    let mut out = format!("{}<Detection>\n", ind(2));
    out += &opt_s("Input_file", s("input_file").as_deref(), d);
    out += &el_s("Start", &time(r.get("t_start")), d);
    out += &opt("End", r.get::<_, Option<DateTime<Utc>>>("t_end"), d, time);
    out += &opt("Count", i("count"), d, |x| x.to_string());
    out += &opt_s("Event", s("event").as_deref(), d);
    out += &opt("UnitId", i("unit_id"), d, |x| x.to_string());
    out += &opt("Channel", i("channel"), d, |x| x.to_string());
    out += &species(r.get("species_tsn"), s("species_group"), d);
    for c in r.get::<_, Option<Vec<String>>>("calls").unwrap_or_default() {
        out += &el_s("Call", &c, d);
    }
    if r.get::<_, bool>("has_parameters") {
        let p = d + 1;
        out += &format!("{}<Parameters>\n", ind(d));
        out += &opt_s("Subtype", s("subtype").as_deref(), p);
        out += &opt("Score", n("score"), p, format_num);
        out += &opt("Confidence", n("confidence"), p, format_num);
        out += &opt_s("QualityAssurance", s("qa").as_deref(), p);
        out += &opt("ReceivedLevel_dB", n("received_level_db"), p, format_num);
        out += &opt_s("FrequencyMeasurements_dB", s("freq_measurements_db").as_deref(), p);
        out += &opt("SNR_dB", n("snr_db"), p, format_num);
        out += &opt("MinFreq_Hz", n("min_freq_hz"), p, format_num);
        out += &opt("MaxFreq_Hz", n("max_freq_hz"), p, format_num);
        out += &opt("PeakFreq_Hz", n("peak_freq_hz"), p, format_num);
        out += &opt_s("Peaks_Hz", s("peaks_hz").as_deref(), p);
        out += &opt("Duration_s", n("duration_s"), p, format_num);
        out += &opt_s("Sideband_Hz", s("sideband_hz").as_deref(), p);
        if r.get::<_, bool>("has_tonal") {
            out += &format!("{}<Tonal>\n", ind(p));
            out += &opt_s("Offset_s", s("tonal_offset_s").as_deref(), p + 1);
            out += &opt_s("Hz", s("tonal_hz").as_deref(), p + 1);
            out += &opt_s("dB", s("tonal_db").as_deref(), p + 1);
            out += &format!("{}</Tonal>\n", ind(p));
        }
        for e in r.get::<_, Option<Vec<String>>>("event_ref").unwrap_or_default() {
            out += &el_s("EventRef", &e, p);
        }
        out += &block(
            "UserDefined",
            r.get::<_, Option<Value>>("user_defined").as_ref(),
            s("user_defined_xml").as_deref(),
            ns,
            p,
        );
        out += &format!("{}</Parameters>\n", ind(d));
    }
    out += &opt_s("Image", s("image").as_deref(), d);
    out += &opt_s("Audio", s("audio").as_deref(), d);
    out += &opt_s("Comment", s("comment").as_deref(), d);
    out += &format!("{}</Detection>\n", ind(2));
    out
}

async fn effort(c: &Client, set_id: i64, e: &Row) -> Result<String> {
    let d = 2;
    let mut out = format!("{IND}<Effort>\n");
    out += &el_s("Start", &time(e.get("t_start")), d);
    out += &el_s("End", &time(e.get("t_end")), d);
    let periodic = c
        .query(
            "SELECT t, duration_s, offset_s, interval_s FROM nereus.analysis_gap_periodic
             WHERE set_id = $1 ORDER BY ord",
            &[&set_id],
        )
        .await?;
    let aperiodic = c
        .query(
            "SELECT t_start, t_end, reason FROM nereus.analysis_gap_aperiodic
             WHERE set_id = $1 ORDER BY ord",
            &[&set_id],
        )
        .await?;
    if !periodic.is_empty() || !aperiodic.is_empty() {
        out += &format!("{}<AnalysisGaps>\n", ind(d));
        if !periodic.is_empty() {
            out += &format!("{}<Periodic>\n", ind(3));
            for g in &periodic {
                out += &format!("{}<Regimen>\n", ind(4));
                out += &el_s("TimeStamp", &time(g.get(0)), 5);
                out += &el(
                    "AnalysisDuration_s",
                    Some(&format_num(g.get(1))),
                    5,
                    &[("Offfset_s", g.get::<_, Option<f64>>(2).map(format_num))],
                );
                out += &el_s("AnalysisInterval_s", &format_num(g.get(3)), 5);
                out += &format!("{}</Regimen>\n", ind(4));
            }
            out += &format!("{}</Periodic>\n", ind(3));
        }
        for g in &aperiodic {
            out += &format!("{}<Aperiodic>\n", ind(3));
            out += &el_s("Start", &time(g.get(0)), 4);
            out += &el_s("End", &time(g.get(1)), 4);
            out += &opt_s("Reason", g.get::<_, Option<&str>>(2), 4);
            out += &format!("{}</Aperiodic>\n", ind(3));
        }
        out += &format!("{}</AnalysisGaps>\n", ind(d));
    }
    out += &opt("IntensityReference_uPa", e.get::<_, Option<f64>>("intensity_ref_upa"), d, format_num);

    for k in c
        .query(
            "SELECT species_tsn, species_group, call, has_parameters, subtype, freq_measurements_hz,
                    granularity, bin_size_min, first_bin_start, encounter_gap_min
             FROM nereus.effort_kind WHERE set_id = $1 ORDER BY ord",
            &[&set_id],
        )
        .await?
    {
        out += &format!("{}<Kind>\n", ind(d));
        out += &species(k.get(0), k.get(1), 3);
        out += &opt_s("Call", k.get::<_, Option<&str>>(2), 3);
        if k.get::<_, bool>(3) {
            out += &format!("{}<Parameters>\n", ind(3));
            out += &opt_s("Subtype", k.get::<_, Option<&str>>(4), 4);
            out += &opt("FrequencyMeasurements_Hz", k.get::<_, Option<Vec<f64>>>(5), 4, |v| num_list(&v));
            out += &format!("{}</Parameters>\n", ind(3));
        }
        out += &el(
            "Granularity",
            Some(k.get::<_, &str>(6)),
            3,
            &[
                ("BinSize_min", k.get::<_, Option<f64>>(7).map(format_num)),
                ("FirstBinStart", k.get::<_, Option<DateTime<Utc>>>(8).map(time)),
                ("EncounterGap_min", k.get::<_, Option<f64>>(9).map(format_num)),
            ],
        );
        out += &format!("{}</Kind>\n", ind(d));
    }
    out += &format!("{IND}</Effort>\n");
    Ok(out)
}

/// Write the Detections document `doc_id`. Returns the number of detections.
/// Detections are streamed, so very large documents don't sit in memory.
pub async fn export_detections<W: Write>(c: &Client, doc_id: &str, w: &mut W) -> Result<u64> {
    let s = c
        .query_opt("SELECT * FROM nereus.detection_set WHERE doc_id = $1", &[&doc_id])
        .await?
        .ok_or_else(|| Error::NotFound(format!("Detections {doc_id}")))?;
    let set_id: i64 = s.get("id");
    let e = c
        .query_one("SELECT * FROM nereus.effort WHERE set_id = $1", &[&set_id])
        .await?;
    let ns_owned: Option<String> = s.get("xml_namespace");
    let ns = ns_owned.as_deref();
    let exact: Option<Value> = s.get("exact_xml");
    let blk = |name: &str, col: &str, depth: usize, key: Option<&str>| {
        block(
            name,
            s.get::<_, Option<Value>>(col).as_ref(),
            exact_of(&exact, key.unwrap_or(name)),
            ns,
            depth,
        )
    };

    let mut head = root_open("Detections", ns, s.get::<_, Option<Value>>("root_attrs").as_ref());
    head += &el_s("Id", &s.get::<_, String>("doc_id"), 1);
    head += &blk("Description", "description", 1, None);
    head += &format!("{IND}<DataSource>\n");
    head += &opt_s("EnsembleId", e.get::<_, Option<&str>>("ensemble_ref"), 2);
    head += &opt_s("DeploymentId", e.get::<_, Option<&str>>("deployment_ref"), 2);
    head += &format!("{IND}</DataSource>\n{IND}<Algorithm>\n");
    head += &opt_s("Method", s.get::<_, Option<&str>>("algorithm_method"), 2);
    head += &opt_s("Software", s.get::<_, Option<&str>>("algorithm_software"), 2);
    head += &opt_s("Version", s.get::<_, Option<&str>>("algorithm_version"), 2);
    head += &blk("Parameters", "algorithm_parameters", 2, Some("Algorithm/Parameters"));
    match exact.as_ref().and_then(|x| x.get("Algorithm/SupportSoftware")) {
        Some(Value::Array(frags)) => {
            for f in frags {
                head += &frag(f.as_str(), 2);
            }
        }
        _ => {
            if let Some(Value::Array(list)) = s.get::<_, Option<Value>>("algorithm_support") {
                for v in &list {
                    head += &block("SupportSoftware", Some(v), None, ns, 2);
                }
            }
        }
    }
    head += &format!("{IND}</Algorithm>\n");
    head += &blk("QualityAssurance", "quality_assurance", 1, None);
    head += &opt_s("UserId", s.get::<_, Option<&str>>("user_id"), 1);
    head += &effort(c, set_id, &e).await?;
    w.write_all(head.as_bytes())?;

    let mut n = 0u64;
    let mut groups = vec![("OnEffort", true)];
    if s.get::<_, bool>("has_offeffort") {
        groups.push(("OffEffort", false));
    }
    let stmt = c.prepare(DETECTION_SELECT).await?;
    for (group, flag) in groups {
        w.write_all(format!("{IND}<{group}>\n").as_bytes())?;
        let params: [&(dyn ToSql + Sync); 2] = [&set_id, &flag];
        let rows = c.query_raw(&stmt, params).await?;
        pin_mut!(rows);
        while let Some(r) = rows.try_next().await? {
            w.write_all(detection(&r, ns).as_bytes())?;
            n += 1;
        }
        w.write_all(format!("{IND}</{group}>\n").as_bytes())?;
    }
    let mut tail = blk("BespokeData", "bespoke_data", 1, None);
    tail += &blk("MetadataInfo", "metadata_info", 1, None);
    tail += "</Detections>\n";
    w.write_all(tail.as_bytes())?;
    Ok(n)
}

// ------------------------------------------------------------- Deployment

/// Start and end tags of a jsonb block whose repeated children come from a
/// table (QualityAssurance, whose Quality periods are rows).
fn open_close(name: &str, value: Option<&Value>, exact: Option<&str>, ns: Option<&str>, depth: usize) -> (String, String) {
    let empty = Value::String(String::new());
    let xml = match exact {
        Some(x) => x.to_string(),
        None => block_to_xml(name, value.unwrap_or(&empty), ns),
    };
    let close = format!("</{name}>");
    let head = if let Some(h) = xml.strip_suffix("/>") {
        format!("{h}>")
    } else {
        xml[..xml.len() - close.len()].to_string()
    };
    (format!("{}{head}\n", ind(depth)), format!("{}{close}\n", ind(depth)))
}

fn contact(value: Option<Value>, exact: &Option<Value>, place: &str, ns: Option<&str>, depth: usize) -> String {
    let Some(Value::Object(o)) = value else { return String::new() };
    let Some((name, v)) = o.iter().next() else { return String::new() };
    block(name, Some(v), exact_of(exact, &format!("{place}/{name}")), ns, depth)
}

fn details(r: &Row, prefix: &str, name: &str, exact: &Option<Value>, ns: Option<&str>) -> String {
    let d = 2;
    let num = |k: &str, col: String| opt(k, r.get::<_, Option<f64>>(col.as_str()), d, format_num);
    let mut out = format!("{IND}<{name}>\n");
    out += &num("Longitude", format!("{prefix}_lon"));
    out += &num("Latitude", format!("{prefix}_lat"));
    out += &num("ElevationInstrument_m", format!("{prefix}_elevation_instrument_m"));
    out += &num("DepthInstrument_m", format!("{prefix}_depth_instrument_m"));
    out += &num("Elevation_m", format!("{prefix}_elevation_m"));
    out += &opt("TimeStamp", r.get::<_, Option<DateTime<Utc>>>(format!("t_{prefix}").as_str()), d, time);
    out += &opt("AudioTimeStamp", r.get::<_, Option<DateTime<Utc>>>(format!("t_{prefix}_audio").as_str()), d, time);
    out += &opt_s("Vessel", r.get::<_, Option<&str>>(format!("{prefix}_vessel").as_str()), d);
    out += &contact(r.get(format!("{prefix}_contact").as_str()), exact, name, ns, d);
    out += &format!("{IND}</{name}>\n");
    out
}

/// Write Deployment `deployment_id`. Returns the number of channels.
pub async fn export_deployment<W: Write>(c: &Client, deployment_id: &str, w: &mut W) -> Result<usize> {
    let r = c
        .query_opt(
            "SELECT d.*, p.name AS project, s.name AS site,
                    i.type AS instrument_type, i.instrument_id AS instrument_ref
             FROM nereus.deployment d
             JOIN nereus.project p ON p.id = d.project_id
             LEFT JOIN nereus.site s ON s.id = d.site_id
             LEFT JOIN nereus.instrument i ON i.id = d.instrument_id
             WHERE d.deployment_id = $1",
            &[&deployment_id],
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("Deployment {deployment_id}")))?;
    let dep: i64 = r.get("id");
    let ns_owned: Option<String> = r.get("xml_namespace");
    let ns = ns_owned.as_deref();
    let exact: Option<Value> = r.get("exact_xml");
    let q = |sql: &'static str| async move { c.query(sql, &[&dep]).await };

    let channels = q("SELECT ord, channel_number, sensor_number, t_start, t_end, event_trigger,
                             has_gain, has_duty_cycle
                      FROM nereus.channel WHERE deployment_id = $1 ORDER BY ord")
    .await?;
    let mut regs: HashMap<(&str, i32), Vec<Row>> = HashMap::new();
    for (t, sql) in [
        ("sampling", "SELECT channel_ord, t, sample_rate_khz, sample_bits FROM nereus.channel_sampling
                      WHERE deployment_id = $1 ORDER BY channel_ord, ord"),
        ("gain", "SELECT channel_ord, t, gain_db, gain_rel FROM nereus.channel_gain
                  WHERE deployment_id = $1 ORDER BY channel_ord, ord"),
        ("duty", "SELECT channel_ord, t, duration_s, offset_s, interval_s FROM nereus.channel_duty_cycle
                  WHERE deployment_id = $1 ORDER BY channel_ord, ord"),
    ] {
        for g in q(sql).await? {
            regs.entry((t, g.get(0))).or_default().push(g);
        }
    }
    let quality = q("SELECT t_start, t_end, category, low_hz, high_hz, channels, comment
                     FROM nereus.recording_quality WHERE deployment_id = $1 ORDER BY ord")
    .await?;
    let tracks = q("SELECT ord, track_id FROM nereus.track WHERE deployment_id = $1 ORDER BY ord").await?;
    let mut points: HashMap<i32, Vec<Row>> = HashMap::new();
    for p in q("SELECT track_ord, t, lon, lat, heading_degn, cog_degn, cog_north, speed, sog,
                       pitch_deg, roll_deg, elevation_m, ground_elevation_m
                FROM nereus.track_point WHERE deployment_id = $1 ORDER BY track_ord, ord")
    .await?
    {
        points.entry(p.get(0)).or_default().push(p);
    }
    let sensors = q("SELECT element, ord, number, sensor_ref, x_m, y_m, z_m, name, description,
                            hydrophone_ref, preamp_ref, type, properties
                     FROM nereus.deployment_sensor WHERE deployment_id = $1
                     ORDER BY array_position(ARRAY['Audio','Depth','Sensor'], element), ord")
    .await?;

    let s = |col: &str| r.get::<_, Option<String>>(col);
    let mut o = root_open("Deployment", ns, r.get::<_, Option<Value>>("root_attrs").as_ref());
    o += &el_s("Id", &r.get::<_, String>("deployment_id"), 1);
    o += &block("Description", r.get::<_, Option<Value>>("description").as_ref(), exact_of(&exact, "Description"), ns, 1);
    o += &el_s("Project", &r.get::<_, String>("project"), 1);
    o += &el_s("DeploymentNumber", &r.get::<_, i32>("deployment_number").to_string(), 1);
    o += &opt_s("DeploymentAlias", s("alias").as_deref(), 1);
    o += &opt_s("Site", s("site").as_deref(), 1);
    if let Some(aliases) = r.get::<_, Option<Vec<String>>>("site_aliases") {
        o += &format!("{IND}<SiteAliases>\n");
        for a in &aliases {
            o += &el_s("Site", a, 2);
        }
        o += &format!("{IND}</SiteAliases>\n");
    }
    o += &opt_s("Cruise", s("cruise").as_deref(), 1);
    o += &el_s("Platform", &r.get::<_, String>("platform"), 1);
    o += &opt_s("Region", s("region").as_deref(), 1);
    o += &format!("{IND}<Instrument>\n");
    o += &el_s("Type", s("instrument_type").as_deref().unwrap_or(""), 2);
    o += &el_s("InstrumentId", s("instrument_ref").as_deref().unwrap_or(""), 2);
    o += &opt_s("GeometryType", s("geometry_type").as_deref(), 2);
    o += &format!("{IND}</Instrument>\n");

    o += &format!("{IND}<SamplingDetails>\n");
    for ch in &channels {
        let i: i32 = ch.get(0);
        o += &format!("{}<Channel>\n", ind(2));
        o += &el_s("ChannelNumber", &ch.get::<_, i32>(1).to_string(), 3);
        o += &el_s("SensorNumber", &ch.get::<_, i32>(2).to_string(), 3);
        o += &el_s("Start", &time(ch.get(3)), 3);
        o += &el_s("End", &time(ch.get(4)), 3);
        o += &block(
            "EventTrigger",
            ch.get::<_, Option<Value>>(5).as_ref(),
            exact_of(&exact, &format!("Channel/{i}/EventTrigger")),
            ns,
            3,
        );
        let reg = |t: &'static str| regs.get(&(t, i)).map(Vec::as_slice).unwrap_or_default();
        o += &format!("{}<Sampling>\n", ind(3));
        for g in reg("sampling") {
            o += &format!("{}<Regimen>\n", ind(4));
            o += &el_s("TimeStamp", &time(g.get(1)), 5);
            o += &el_s("SampleRate_kHz", &format_num(g.get(2)), 5);
            o += &el_s("SampleBits", &g.get::<_, i32>(3).to_string(), 5);
            o += &format!("{}</Regimen>\n", ind(4));
        }
        o += &format!("{}</Sampling>\n", ind(3));
        if ch.get::<_, bool>(6) {
            o += &format!("{}<Gain>\n", ind(3));
            for g in reg("gain") {
                o += &format!("{}<Regimen>\n", ind(4));
                o += &el_s("TimeStamp", &time(g.get(1)), 5);
                o += &opt("Gain_dB", g.get::<_, Option<f64>>(2), 5, format_num);
                o += &opt("Gain_rel", g.get::<_, Option<f64>>(3), 5, format_num);
                o += &format!("{}</Regimen>\n", ind(4));
            }
            o += &format!("{}</Gain>\n", ind(3));
        }
        if ch.get::<_, bool>(7) {
            o += &format!("{}<DutyCycle>\n", ind(3));
            for g in reg("duty") {
                o += &format!("{}<Regimen>\n", ind(4));
                o += &el_s("TimeStamp", &time(g.get(1)), 5);
                o += &el(
                    "RecordingDuration_s",
                    Some(&format_num(g.get(2))),
                    5,
                    &[("Offfset_s", g.get::<_, Option<f64>>(3).map(format_num))],
                );
                o += &el_s("RecordingInterval_s", &format_num(g.get(4)), 5);
                o += &format!("{}</Regimen>\n", ind(4));
            }
            o += &format!("{}</DutyCycle>\n", ind(3));
        }
        o += &format!("{}</Channel>\n", ind(2));
    }
    o += &format!("{IND}</SamplingDetails>\n");

    if let Some(qa) = r.get::<_, Option<Value>>("quality_assurance") {
        let (head, tail) = open_close("QualityAssurance", Some(&qa), exact_of(&exact, "QualityAssurance"), ns, 1);
        o += &head;
        for g in &quality {
            o += &format!("{}<Quality>\n", ind(2));
            o += &el_s("Start", &time(g.get(0)), 3);
            o += &el_s("End", &time(g.get(1)), 3);
            o += &el_s("Category", g.get::<_, &str>(2), 3);
            if let Some(low) = g.get::<_, Option<f64>>(3) {
                o += &format!("{}<FrequencyRange>\n", ind(3));
                o += &el_s("Low_Hz", &format_num(low), 4);
                o += &el_s("High_Hz", &format_num(g.get::<_, Option<f64>>(4).unwrap_or(f64::NAN)), 4);
                o += &format!("{}</FrequencyRange>\n", ind(3));
            }
            for c in g.get::<_, Option<Vec<i32>>>(5).unwrap_or_default() {
                o += &el_s("Channel", &c.to_string(), 3);
            }
            o += &opt_s("Comment", g.get::<_, Option<&str>>(6), 3);
            o += &format!("{}</Quality>\n", ind(2));
        }
        o += &tail;
    }

    o += &format!("{IND}<Data>\n{}<Audio>\n", ind(2));
    o += &el_s("URI", s("audio_uri").as_deref().unwrap_or(""), 3);
    o += &opt_s("FurtherInformationURL", s("audio_info_url").as_deref(), 3);
    o += &opt_s("ServiceExpectation", s("audio_service").as_deref(), 3);
    o += &opt_s("Processed", s("audio_processed").as_deref(), 3);
    o += &opt_s("Raw", s("audio_raw").as_deref(), 3);
    o += &format!("{}</Audio>\n", ind(2));
    if r.get::<_, bool>("has_tracks") {
        o += &format!("{}<Tracks>\n", ind(2));
        o += &opt_s("SpeedUnit", s("track_speed_unit").as_deref(), 3);
        for t in &tracks {
            o += &format!("{}<Track>\n", ind(3));
            o += &opt("TrackId", t.get::<_, Option<f64>>(1), 4, format_num);
            for p in points.get(&t.get::<_, i32>(0)).map(Vec::as_slice).unwrap_or_default() {
                let n = |k: &str, i: usize| opt(k, p.get::<_, Option<f64>>(i), 5, format_num);
                o += &format!("{}<Point>\n", ind(4));
                o += &el_s("TimeStamp", &time(p.get(1)), 5);
                o += &n("Longitude", 2);
                o += &n("Latitude", 3);
                o += &n("Heading_DegN", 4);
                if let Some(cog) = p.get::<_, Option<f64>>(5) {
                    o += &el("CourseOverGround_DegN", Some(&format_num(cog)), 5, &[("north", p.get(6))]);
                }
                o += &n("Speed", 7);
                o += &n("SpeedOverGround", 8);
                o += &n("Pitch_deg", 9);
                o += &n("Roll_deg", 10);
                o += &n("Elevation_m", 11);
                o += &n("GroundElevation_m", 12);
                o += &format!("{}</Point>\n", ind(4));
            }
            o += &format!("{}</Track>\n", ind(3));
        }
        o += &block("TrackEffort", r.get::<_, Option<Value>>("track_effort").as_ref(), exact_of(&exact, "TrackEffort"), ns, 3);
        for u in r.get::<_, Option<Vec<String>>>("track_uris").unwrap_or_default() {
            o += &el_s("URI", &u, 3);
        }
        o += &opt_s("FurtherInformationURL", s("track_info_url").as_deref(), 3);
        o += &opt_s("ServiceExpectation", s("track_service").as_deref(), 3);
        o += &format!("{}</Tracks>\n", ind(2));
    }
    o += &format!("{IND}</Data>\n");

    o += &details(&r, "deploy", "DeploymentDetails", &exact, ns);
    if r.get::<_, Option<DateTime<Utc>>>("t_recover").is_some() {
        o += &details(&r, "recover", "RecoveryDetails", &exact, ns);
    }

    o += &format!("{IND}<Sensors>\n");
    o += &opt_s("ReferencePoint", s("sensor_reference_point").as_deref(), 2);
    for x in &sensors {
        let (element, i): (&str, i32) = (x.get(0), x.get(1));
        o += &format!("{}<{element}>\n", ind(2));
        o += &el_s("Number", &x.get::<_, i32>(2).to_string(), 3);
        o += &el_s("SensorId", x.get::<_, &str>(3), 3);
        if let Some(xm) = x.get::<_, Option<f64>>(4) {
            o += &format!("{}<Geometry>\n", ind(3));
            o += &el_s("x_m", &format_num(xm), 4);
            o += &opt("y_m", x.get::<_, Option<f64>>(5), 4, format_num);
            o += &opt("z_m", x.get::<_, Option<f64>>(6), 4, format_num);
            o += &format!("{}</Geometry>\n", ind(3));
        }
        o += &opt_s("Name", x.get::<_, Option<&str>>(7), 3);
        o += &opt_s("Description", x.get::<_, Option<&str>>(8), 3);
        o += &opt_s("HydrophoneId", x.get::<_, Option<&str>>(9), 3);
        o += &opt_s("PreampId", x.get::<_, Option<&str>>(10), 3);
        o += &opt_s("Type", x.get::<_, Option<&str>>(11), 3);
        o += &block(
            "Properties",
            x.get::<_, Option<Value>>(12).as_ref(),
            exact_of(&exact, &format!("Sensors/Sensor/{i}/Properties")),
            ns,
            3,
        );
        o += &format!("{}</{element}>\n", ind(2));
    }
    o += &format!("{IND}</Sensors>\n");
    o += &block("MetadataInfo", r.get::<_, Option<Value>>("metadata_info").as_ref(), exact_of(&exact, "MetadataInfo"), ns, 1);
    o += "</Deployment>\n";
    w.write_all(o.as_bytes())?;
    Ok(channels.len())
}
