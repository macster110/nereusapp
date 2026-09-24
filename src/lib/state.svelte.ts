// App-wide state: the connection, the deployments, filters and selection.

import { api, message, type DbInfo, type Deployment, type SpeciesName, type Track } from "./api";

export type ColorBy = "project" | "instrument" | "platform";
export type Basemap = "dark" | "satellite" | "ocean";

export interface Area {
  south: number;
  north: number;
  /** West and east edges; west > east means the box crosses the antimeridian. */
  west: number;
  east: number;
}

// Categorical palette (dark-surface steps), in fixed order. Slot 8 is left
// for "Other" in grey; categories past seven fold into it.
export const PALETTE = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#9085e9", "#e66767"];
export const OTHER = "#8a94a6";

export function inArea(a: Area, lat: number, lon: number): boolean {
  if (lat < a.south || lat > a.north) return false;
  return a.west <= a.east ? lon >= a.west && lon <= a.east : lon >= a.west || lon <= a.east;
}

function category(d: Deployment, by: ColorBy): string {
  if (by === "project") return d.project;
  if (by === "instrument") return d.instrument_type ?? "Unknown";
  return d.platform;
}

class Store {
  info = $state<DbInfo | null>(null);
  deployments = $state<Deployment[]>([]);
  tracks = $state<Track[]>([]);
  species = $state<Record<string, SpeciesName>>({});
  loading = $state(false);
  error = $state<string | null>(null);

  // Filters. Deployments are kept if they overlap [t0, t1].
  text = $state("");
  t0 = $state<number | null>(null);
  t1 = $state<number | null>(null);
  instruments = $state<string[]>([]); // empty = all
  area = $state<Area | null>(null);
  onlyWithDetections = $state(false);

  colorBy = $state<ColorBy>("project");
  basemap = $state<Basemap>("dark");
  selectedId = $state<number | null>(null);
  drawingArea = $state(false);

  showConnect = $state(true);
  showExport = $state(false);
  showSettings = $state(false);
  exportScope = $state<"selected" | "filtered">("filtered");
  timelineOpen = $state(true);

  byId = $derived(new Map(this.deployments.map((d) => [d.id, d])));
  selected = $derived(this.selectedId == null ? null : (this.byId.get(this.selectedId) ?? null));

  instrumentTypes = $derived.by(() => {
    const n = new Map<string, number>();
    for (const d of this.deployments) {
      const k = d.instrument_type ?? "Unknown";
      n.set(k, (n.get(k) ?? 0) + 1);
    }
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  });

  filtered = $derived.by(() => {
    const q = this.text.trim().toLowerCase();
    const types = new Set(this.instruments);
    return this.deployments.filter((d) => {
      if (types.size && !types.has(d.instrument_type ?? "Unknown")) return false;
      if (this.onlyWithDetections && d.n_detections === 0) return false;
      const end = d.t_recover ?? d.t_deploy;
      if (this.t0 != null && end < this.t0) return false;
      if (this.t1 != null && d.t_deploy >= this.t1) return false;
      if (this.area && (d.lat == null || d.lon == null || !inArea(this.area, d.lat, d.lon))) return false;
      if (q) {
        const hay = `${d.deployment_id} ${d.project} ${d.site ?? ""} ${d.region ?? ""} ${d.instrument_id ?? ""}`;
        if (!hay.toLowerCase().includes(q)) return false;
      }
      return true;
    });
  });

  filtersActive = $derived(
    this.text.trim() !== "" || this.t0 != null || this.t1 != null || this.instruments.length > 0 ||
      this.area != null || this.onlyWithDetections,
  );

  /** Colour per category, ranked over all deployments so filtering never repaints. */
  legend = $derived.by(() => {
    const n = new Map<string, number>();
    for (const d of this.deployments) {
      const k = category(d, this.colorBy);
      n.set(k, (n.get(k) ?? 0) + 1);
    }
    const ranked = [...n.entries()].sort((a, b) => b[1] - a[1]);
    const colors = new Map<string, string>();
    ranked.forEach(([k], i) => colors.set(k, i < PALETTE.length ? PALETTE[i] : OTHER));
    const shown = ranked.slice(0, PALETTE.length).map(([k, c]) => ({ label: k, color: colors.get(k)!, n: c }));
    const rest = ranked.slice(PALETTE.length);
    if (rest.length)
      shown.push({ label: `Other (${rest.length})`, color: OTHER, n: rest.reduce((s, [, c]) => s + c, 0) });
    return { colors, items: shown };
  });

  colorOf(d: Deployment): string {
    return this.legend.colors.get(category(d, this.colorBy)) ?? OTHER;
  }

  speciesName(tsn: number): string {
    // Tethys files anthropogenic sounds under Homo sapiens, whose ITIS name is "man".
    if (tsn === 180092) return "Human activity";
    const n = this.species[String(tsn)];
    if (n?.common) return n.common.replace(/^./, (c) => c.toUpperCase());
    if (n?.scientific) return n.scientific;
    return tsn < 0 ? `Tethys code ${tsn}` : `TSN ${tsn}`;
  }

  speciesLatin(tsn: number): string | null {
    const n = this.species[String(tsn)];
    return n?.common && n.scientific ? n.scientific : null;
  }

  clearFilters() {
    this.text = "";
    this.t0 = this.t1 = null;
    this.instruments = [];
    this.area = null;
    this.onlyWithDetections = false;
  }

  async load(info: DbInfo) {
    this.info = info;
    this.loading = true;
    this.error = null;
    this.selectedId = null;
    try {
      const [deps, tracks] = await Promise.all([api.deployments(), api.tracks()]);
      this.deployments = deps;
      this.tracks = tracks;
    } catch (e) {
      this.error = message(e);
    } finally {
      this.loading = false;
    }
    // Names come from ITIS over the network; the map doesn't wait for them.
    const tsns = [...new Set(this.deployments.flatMap((d) => d.species))];
    if (tsns.length) {
      api.speciesNames(tsns).then((s) => (this.species = { ...this.species, ...s })).catch(() => {});
    }
  }

  async disconnect() {
    await api.disconnect();
    this.info = null;
    this.deployments = [];
    this.tracks = [];
    this.selectedId = null;
    this.clearFilters();
    this.showConnect = true;
  }
}

export const app = new Store();
