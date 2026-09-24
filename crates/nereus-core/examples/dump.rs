//! Developer tool for checking exports against a real database.
//!
//!   cargo run --example dump -- xml  DSN OUTDIR        every document as Tethys XML
//!   cargo run --example dump -- data DSN OUTDIR [N]    .mat/.RData/zip for N deployments
//!   cargo run --example dump -- browse DSN             time the queries behind the UI

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use nereus_core::export::{ExportRequest, Selection};
use nereus_core::{browse, export, tethys, ConnectionSettings, Db};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, dsn) = (args[0].as_str(), &args[1]);
    let (db, info) = Db::connect(ConnectionSettings::parse(dsn)?).await?;
    eprintln!("{info:?}");
    let c = db.pool.get().await?;
    match cmd {
        "xml" => {
            let out = std::path::Path::new(&args[2]);
            std::fs::create_dir_all(out.join("Deployments"))?;
            std::fs::create_dir_all(out.join("Detections"))?;
            let t = Instant::now();
            for r in c.query("SELECT deployment_id FROM nereus.deployment ORDER BY 1", &[]).await? {
                let id: String = r.get(0);
                let mut f = std::io::BufWriter::new(std::fs::File::create(out.join("Deployments").join(format!("{id}.xml")))?);
                tethys::export_deployment(&c, &id, &mut f).await?;
            }
            let mut n = 0;
            for r in c.query("SELECT doc_id FROM nereus.detection_set ORDER BY 1", &[]).await? {
                let id: String = r.get(0);
                let mut f = std::io::BufWriter::new(std::fs::File::create(out.join("Detections").join(format!("{id}.xml")))?);
                n += tethys::export_detections(&c, &id, &mut f).await?;
            }
            eprintln!("{n} detections exported in {:.1?}", t.elapsed());
        }
        "data" => {
            let n: i64 = args.get(3).map_or(5, |s| s.parse().unwrap());
            let ids: Vec<i64> = c
                .query(
                    "SELECT deployment_id FROM nereus.summary_daily WHERE deployment_id IS NOT NULL
                     GROUP BY 1 ORDER BY sum(n_detections) DESC LIMIT $1",
                    &[&n],
                )
                .await?
                .iter()
                .map(|r| r.get(0))
                .collect();
            let req = ExportRequest {
                selection: Selection { deployment_ids: ids, t0: None, t1: None },
                matlab: true,
                r: true,
                tethys: true,
                raw: true,
                out_dir: args[2].clone().into(),
                name: "dump".into(),
            };
            eprintln!("{:?}", export::preview(&db, &req.selection).await?);
            let t = Instant::now();
            let res = export::run(&db, &req, &|p| eprintln!("  {} {}/{} {}", p.stage, p.done, p.total, p.detail), &AtomicBool::new(false)).await?;
            eprintln!("{res:?} in {:.1?}", t.elapsed());
        }
        "browse" => {
            let t = Instant::now();
            let deps = browse::deployments(&db).await?;
            eprintln!("{} deployments in {:.1?}", deps.len(), t.elapsed());
            let t = Instant::now();
            let tracks = browse::tracks(&db).await?;
            eprintln!("{} tracks, {} points in {:.1?}", tracks.len(), tracks.iter().map(|t| t.points.len()).sum::<usize>(), t.elapsed());
            let busiest = deps.iter().max_by_key(|d| d.n_detections).unwrap();
            eprintln!("busiest: {} ({} detections)", busiest.deployment_id, busiest.n_detections);
            let t = Instant::now();
            let meta = browse::timeline_meta(&db, &[busiest.id]).await?;
            eprintln!("meta: {} lanes, {} effort spans in {:.1?}", meta.lanes.len(), meta.effort.len(), t.elapsed());
            for (label, span) in [("all", meta.t_max - meta.t_min), ("1 day", 86_400.0), ("1 hour", 3_600.0)] {
                let t = Instant::now();
                let d = browse::timeline_data(&db, &[busiest.id], meta.t_min, meta.t_min + span, 800).await?;
                eprintln!("{label}: {} ({} bins, {} events) in {:.1?}", d.mode, d.bin.len(), d.start.len(), t.elapsed());
            }
            let t = Instant::now();
            let det = browse::deployment_detail(&db, busiest.id).await?;
            eprintln!("detail: {} sets in {:.1?}", det.detection_sets.len(), t.elapsed());
        }
        _ => eprintln!("unknown command"),
    }
    Ok(())
}
