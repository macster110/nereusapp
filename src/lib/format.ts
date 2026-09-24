// Display helpers. All times are shown in UTC, as PAM data are.

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number, w = 2) => String(n).padStart(w, "0");

/** 12 Mar 2019 */
export function date(t: number | null | undefined): string {
  if (t == null) return "—";
  const d = new Date(t * 1000);
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}

/** 12 Mar 2019 14:05 */
export function dateTime(t: number | null | undefined): string {
  if (t == null) return "—";
  const d = new Date(t * 1000);
  return `${date(t)} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** 14:05:09.250 */
export function timeOfDay(t: number, ms = false): string {
  const d = new Date(t * 1000);
  const s = `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
  return ms ? `${s}.${pad(d.getUTCMilliseconds(), 3)}` : s;
}

/** yyyy-mm-dd for <input type="date">. */
export function isoDay(t: number): string {
  const d = new Date(t * 1000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
}

/** Seconds since 1970 from yyyy-mm-dd (UTC midnight), or null. */
export function parseDay(s: string): number | null {
  if (!s) return null;
  const t = Date.parse(`${s}T00:00:00Z`);
  return Number.isNaN(t) ? null : t / 1000;
}

export function duration(seconds: number): string {
  const d = seconds / 86400;
  if (d >= 365) return `${(d / 365.25).toFixed(1)} years`;
  if (d >= 2) return `${Math.round(d)} days`;
  const h = seconds / 3600;
  if (h >= 2) return `${Math.round(h)} hours`;
  return `${Math.round(seconds / 60)} min`;
}

export function count(n: number): string {
  return n.toLocaleString("en-GB");
}

/** 1.2k, 3.4M */
export function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1e6) return `${(n / 1e3).toFixed(n < 1e4 ? 1 : 0)}k`;
  return `${(n / 1e6).toFixed(n < 1e7 ? 1 : 0)}M`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let i = -1;
  do {
    n /= 1024;
    i++;
  } while (n >= 1024 && i < u.length - 1);
  return `${n.toFixed(n < 10 ? 1 : 0)} ${u[i]}`;
}

export function latLon(lat: number | null, lon: number | null): string {
  if (lat == null || lon == null) return "—";
  return `${Math.abs(lat).toFixed(3)}° ${lat >= 0 ? "N" : "S"}, ${Math.abs(lon).toFixed(3)}° ${lon >= 0 ? "E" : "W"}`;
}
