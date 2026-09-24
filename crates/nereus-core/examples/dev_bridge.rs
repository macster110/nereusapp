//! Development only: serves the app's commands over HTTP so the UI can run
//! in an ordinary browser (`npm run dev`, then open http://localhost:1420).
//! The desktop app never uses this; it talks to Rust through Tauri.
//!
//!   cargo run --example dev_bridge
//!
//! Listens on 127.0.0.1:1421. POST /invoke/<command> with the command's
//! arguments as JSON, as Tauri's invoke() would pass them.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use nereus_core::export::{ExportRequest, Selection};
use nereus_core::{browse, export, species, ConnectionSettings, Db};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

type State = Arc<RwLock<Option<Db>>>;

fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

async fn dispatch(state: &State, cmd: &str, args: Value) -> Result<Value, String> {
    let e = |x: nereus_core::Error| x.to_string();
    if cmd == "connect" {
        let (db, info) = Db::connect(arg(&args, "settings")?).await.map_err(e)?;
        *state.write().await = Some(db);
        return Ok(json!(info));
    }
    if cmd == "parse_connection_string" {
        return Ok(json!(ConnectionSettings::parse(&arg::<String>(&args, "url")?).map_err(e)?));
    }
    if cmd == "species_names" {
        let cache = std::env::temp_dir().join("nereus_itis_species_v2.json");
        return Ok(json!(species::names(&arg::<Vec<i64>>(&args, "tsns")?, &cache).await));
    }
    if cmd == "disconnect" {
        *state.write().await = None;
        return Ok(Value::Null);
    }
    let db = state.read().await.clone().ok_or("not connected to a database")?;
    Ok(match cmd {
        "deployments" => json!(browse::deployments(&db).await.map_err(e)?),
        "tracks" => json!(browse::tracks(&db).await.map_err(e)?),
        "deployment_detail" => json!(browse::deployment_detail(&db, arg(&args, "id")?).await.map_err(e)?),
        "timeline_meta" => json!(browse::timeline_meta(&db, &arg::<Vec<i64>>(&args, "ids")?).await.map_err(e)?),
        "timeline_data" => json!(browse::timeline_data(
            &db,
            &arg::<Vec<i64>>(&args, "ids")?,
            arg(&args, "t0")?,
            arg(&args, "t1")?,
            arg(&args, "bins")?
        )
        .await
        .map_err(e)?),
        "export_preview" => json!(export::preview(&db, &arg::<Selection>(&args, "selection")?).await.map_err(e)?),
        "start_export" => {
            let req: ExportRequest = arg(&args, "request")?;
            json!(export::run(&db, &req, &|_| {}, &AtomicBool::new(false)).await.map_err(e)?)
        }
        "cancel_export" => Value::Null,
        other => return Err(format!("unknown command {other}")),
    })
}

async fn handle(state: State, mut sock: tokio::net::TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let head_end = loop {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let len: usize = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let n = sock.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (status, body) = if method == "OPTIONS" {
        ("204 No Content", String::new())
    } else if let Some(cmd) = path.strip_prefix("/invoke/") {
        let args: Value = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
        match dispatch(&state, cmd, args).await {
            Ok(v) => ("200 OK", v.to_string()),
            Err(m) => ("500 Internal Server Error", Value::String(m).to_string()),
        }
    } else {
        ("404 Not Found", "\"not found\"".into())
    };
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(resp.as_bytes()).await?;
    sock.shutdown().await
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    nereus_core::db::install_crypto();
    let listener = TcpListener::bind("127.0.0.1:1421").await?;
    eprintln!("dev bridge on http://127.0.0.1:1421 (open the UI at http://localhost:1420)");
    let state: State = Arc::default();
    loop {
        let (sock, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(state, sock).await {
                eprintln!("{e}");
            }
        });
    }
}
