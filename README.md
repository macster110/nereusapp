# Nereus

A desktop app for exploring passive acoustic monitoring data in a
[Nereus](https://github.com/macster110/Nereus) database. It runs on Windows, macOS and Linux.

* **Globe.** Every deployment is drawn on a 3D globe: moorings as points,
  drifters, gliders and towed arrays as tracks. Points are coloured by
  project, instrument or platform. Faded points have no detections.
* **Filters.** Keep it simple: search by text, a time period, instrument
  type, an area drawn on the globe, or only deployments with detections.
  Anything more complex belongs in the Python, R or MATLAB libraries.
* **Timeline.** Select a deployment with detections and a timeline opens
  under the globe, with one row per species and call type. Grey bands show
  analysis effort: where someone looked. Zoomed out it shows counts per time
  bin; zoomed in, individual detections. Scroll to zoom, drag to pan, and
  double-click to zoom in. The keyboard also works: `+`, `-`, `←`, `→`,
  and `0` to fit.
* **Downloads.** Download the selected deployment, or everything matching the
  filters, as:
  * **MATLAB** `.mat`: a `deployments` struct array. Each element has the
    deployment's fields plus `detections` and `effort` as structs of column
    vectors. Times are datenums (UTC), and text is cell arrays of char.
  * **R** `.RData`: `deployments`, `effort` and `detections` data frames,
    plus `nereus_info`. `load()` the file. Times are POSIXct in UTC.
  * **Tethys XML** `.zip`: the Deployment and Detections documents,
    identical to what the Python exporter (`nereus.cli export`) writes.
  * **Raw data files**: audio, PAMGuard binaries, CPOD/FPOD files and
    anything else registered in `nereus.data_file`, copied with their folder
    structure. `file://` (local or network drives) and `http(s)://`
    storage work now. `s3://` storage is listed but not downloaded yet.

Species names come from ITIS, looked up once and then cached.

## Install

Installers are attached to each [release](../../releases): `.msi` or `.exe`
for Windows, `.dmg` for macOS, and `.AppImage`, `.deb` or `.rpm` for
Linux. Until the builds are code-signed, Windows SmartScreen and macOS
Gatekeeper warn on first launch. On macOS, right-click the app and choose
**Open**.

The app connects directly to PostgreSQL, so it needs network access to the
database. A read-only role is all it needs, for example the
`nereus_reader` role from the Nereus schema.

## Architecture

```
crates/nereus-core   Rust library: database access, queries, and the MATLAB,
                     R and Tethys XML writers. Independent of the UI.
src-tauri            The desktop app: Tauri commands over nereus-core.
src                  The UI: Svelte 5 + TypeScript, Globe.gl, and a canvas
                     timeline.
```

The UI calls Rust through Tauri commands (`src/lib/api.ts` ↔
`src-tauri/src/lib.rs`). Times cross that boundary as seconds since 1970
(UTC).

Notes on the export formats:

* `tethys/` is a port of the Python exporters (`nereus/export.py`,
  `nereus/deployment.py`, `nereus/xmljson.py`). It is checked byte for byte
  against them on the whole Tethys demo database (858 Deployments and 299
  Detections documents, 3.1 GB) and on the kitchen-sink fixtures. The Rust
  export runs about 2.6× faster. Numbers are written the way Python's
  `repr()` writes them, and JSON keeps its key order (`serde_json`
  `preserve_order`), so the documents are identical.
* `mat.rs` writes Level 5 MAT-files, with each variable zlib-compressed.
  Loading was checked in MATLAB R2022b.
* `rdata.rs` writes R serialization version 3 (XDR, gzipped). Loading was
  checked in R 4.2.2.

## Development

You need Rust (stable), Node.js (LTS) and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
system:

* **Windows:** the MSVC C++ build tools. WebView2 ships with Windows 11.
* **macOS:** Xcode command line tools.
* **Linux:** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`,
  `librsvg2-dev`, `patchelf`.

```bash
npm install
npm run tauri dev      # the app, with hot reload
npm run tauri build    # installers for this platform, in target/release/bundle
cargo test --workspace
npm run check          # Svelte/TypeScript type check
```

GitHub Actions builds installers for every platform when you push a tag like
`v0.1.0` (`.github/workflows/release.yml`). The builds land in a draft
release.

### Working on the UI in a browser

The UI can also run in an ordinary browser, which is handy for layout work
and automated testing. Start a small HTTP bridge to the same Rust code, then
start the Vite dev server:

```bash
cargo run --example dev_bridge   # serves the commands on 127.0.0.1:1421
npm run dev                      # then open http://localhost:1420
```

The desktop app never uses the bridge. Native dialogs (folder picker,
"Show in folder") only work in the app itself.

### Checking exports against a database

```bash
cargo run --release --example dump -- browse postgresql://user@host/db   # time the UI queries
cargo run --release --example dump -- xml  postgresql://user@host/db OUT # every document as XML
cargo run --release --example dump -- data postgresql://user@host/db OUT 5
```

## Not done yet

* Uploading datasets.
* Downloading from `s3://` storage.
* Remembering passwords (in the OS keychain).
* Code signing for the macOS and Windows installers.
* Whistle contours in the MATLAB and R exports. They are in the Tethys XML.

## Licence

Nereus is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE).
