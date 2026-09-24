// Typed wrappers around the Rust commands (src-tauri/src/lib.rs).
// Times are seconds since 1970, UTC.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type SslMode = "disable" | "prefer" | "require" | "verify";

export interface ConnectionSettings {
  host: string;
  port: number;
  database: string;
  user: string;
  password?: string | null;
  ssl: SslMode;
}

export interface DbInfo {
  label: string;
  server_version: string;
  n_deployments: number;
  n_detection_sets: number;
  n_detections_estimate: number;
  n_data_files: number;
}

export interface Deployment {
  id: number;
  deployment_id: string;
  project: string;
  site: string | null;
  region: string | null;
  platform: string;
  deployment_number: number;
  instrument_type: string | null;
  instrument_id: string | null;
  lon: number | null;
  lat: number | null;
  depth_m: number | null;
  t_deploy: number;
  t_recover: number | null;
  has_tracks: boolean;
  sample_rate_khz: number | null;
  n_detection_sets: number;
  n_detections: number;
  species: number[];
  n_files: number;
  file_bytes: number;
}

export interface Track {
  deployment: number;
  points: [number, number][];
}

export interface Kind {
  species_tsn: number;
  call: string | null;
  granularity: string;
  bin_size_s: number | null;
}

export interface DetectionSet {
  doc_id: string;
  method: string | null;
  software: string | null;
  version: string | null;
  user_id: string | null;
  t_start: number;
  t_end: number;
  n_detections: number;
  kinds: Kind[];
}

export interface DeploymentDetail {
  id: number;
  alias: string | null;
  cruise: string | null;
  site_aliases: string[];
  abstract_text: string | null;
  objectives: string | null;
  n_channels: number;
  sample_rates_khz: number[];
  duty_cycled: boolean;
  audio_uri: string | null;
  detection_sets: DetectionSet[];
  files: { format: string; role: string; n: number; bytes: number }[];
}

export interface Lane {
  key: string;
  species_tsn: number;
  call: string;
  n_detections: number;
}

export interface Span {
  lane: string;
  start: number;
  end: number;
}

export interface TimelineMeta {
  lanes: Lane[];
  effort: Span[];
  recording: Span[];
  t_min: number;
  t_max: number;
}

export interface TimelineData {
  mode: "bins" | "events";
  t0: number;
  t1: number;
  n_bins: number;
  keys: string[];
  bin: number[];
  bin_key: number[];
  count: number[];
  start: number[];
  end: number[];
  event_key: number[];
}

export interface SpeciesName {
  scientific: string | null;
  common: string | null;
}

export interface Selection {
  deployment_ids: number[];
  t0: number | null;
  t1: number | null;
}

export interface ExportPreview {
  n_deployments: number;
  n_detections: number;
  n_documents: number;
  n_files: number;
  file_bytes: number;
  n_files_unsupported: number;
}

export interface ExportRequest extends Selection {
  matlab: boolean;
  r: boolean;
  tethys: boolean;
  raw: boolean;
  out_dir: string;
  name: string;
}

export interface ExportResult {
  written: string[];
  n_detections: number;
  warnings: string[];
}

export interface Progress {
  stage: string;
  done: number;
  total: number;
  detail: string;
}

/** Running inside the desktop app (not a plain browser during UI development). */
export const inTauri = "__TAURI_INTERNALS__" in window;

// In a browser, commands go to the development bridge
// (cargo run --example dev_bridge); the desktop app never does this.
async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (inTauri) return invoke<T>(cmd, args);
  const r = await fetch(`http://127.0.0.1:1421/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) });
  const body = await r.json();
  if (!r.ok) throw body;
  return body as T;
}

export const api = {
  connect: (settings: ConnectionSettings) => call<DbInfo>("connect", { settings }),
  disconnect: () => call<void>("disconnect"),
  parseConnectionString: (url: string) => call<ConnectionSettings>("parse_connection_string", { url }),
  deployments: () => call<Deployment[]>("deployments"),
  tracks: () => call<Track[]>("tracks"),
  deploymentDetail: (id: number) => call<DeploymentDetail>("deployment_detail", { id }),
  timelineMeta: (ids: number[]) => call<TimelineMeta>("timeline_meta", { ids }),
  timelineData: (ids: number[], t0: number, t1: number, bins: number) =>
    call<TimelineData>("timeline_data", { ids, t0, t1, bins }),
  speciesNames: (tsns: number[]) => call<Record<string, SpeciesName>>("species_names", { tsns }),
  exportPreview: (selection: Selection) => call<ExportPreview>("export_preview", { selection }),
  startExport: (request: ExportRequest) => call<ExportResult>("start_export", { request }),
  cancelExport: () => call<void>("cancel_export"),
  onExportProgress: async (f: (p: Progress) => void): Promise<UnlistenFn> =>
    inTauri ? listen<Progress>("export-progress", (e) => f(e.payload)) : () => {},
};

/** Error text from a failed command. */
export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
