<script lang="ts">
  import { onMount } from "svelte";
  import { api, message, type TimelineData, type TimelineMeta } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import * as fmt from "../lib/format";

  let {
    ids,
    title,
    preferredHeight = $bindable(0),
  }: { ids: number[]; title: string; preferredHeight?: number } = $props();

  let meta = $state<TimelineMeta | null>(null);
  let data = $state<TimelineData | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let view = $state({ t0: 0, t1: 1 });
  let hover = $state<{ x: number; y: number; html: string } | null>(null);

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(800);
  let height = $state(200);

  const GUTTER = 240;
  const AXIS = 26;
  const RIGHT = 14;
  const MIN_ROW = 20;
  const MAX_ROW = 30;
  const MIN_SPAN = 10; // seconds

  const lanes = $derived(meta?.lanes ?? []);
  const rowH = $derived(
    lanes.length ? Math.max(MIN_ROW, Math.min(MAX_ROW, (height - AXIS - 6) / lanes.length)) : MAX_ROW,
  );
  const canvasH = $derived(Math.max(height, AXIS + lanes.length * rowH + 6));
  const plotW = $derived(Math.max(50, width - GUTTER - RIGHT));

  // Height the drawer needs to show every lane at full size (header included).
  $effect(() => {
    preferredHeight = meta ? 41 + AXIS + Math.max(2, lanes.length) * MAX_ROW + 10 : 0;
  });

  const x = (t: number) => GUTTER + ((t - view.t0) / (view.t1 - view.t0)) * plotW;
  const tAt = (px: number) => view.t0 + ((px - GUTTER) / plotW) * (view.t1 - view.t0);

  function fullRange(): { t0: number; t1: number } {
    if (!meta || meta.t_max <= meta.t_min) return { t0: Date.now() / 1000 - 86400, t1: Date.now() / 1000 };
    const pad = (meta.t_max - meta.t_min) * 0.02;
    return { t0: meta.t_min - pad, t1: meta.t_max + pad };
  }

  function setView(t0: number, t1: number) {
    const full = fullRange();
    const maxSpan = (full.t1 - full.t0) * 1.5;
    let span = Math.min(Math.max(t1 - t0, MIN_SPAN), maxSpan);
    const mid = (t0 + t1) / 2;
    let a = mid - span / 2;
    // Keep at least part of the data in view.
    a = Math.min(Math.max(a, full.t0 - span * 0.9), full.t1 - span * 0.1);
    view = { t0: a, t1: a + span };
  }

  export function zoom(factor: number, at?: number) {
    const c = at ?? (view.t0 + view.t1) / 2;
    setView(c - (c - view.t0) * factor, c + (view.t1 - c) * factor);
  }

  export function pan(fraction: number) {
    const d = (view.t1 - view.t0) * fraction;
    setView(view.t0 + d, view.t1 + d);
  }

  export function fit() {
    const f = fullRange();
    view = f;
  }

  // New selection: load lanes and effort, show everything.
  let metaSeq = 0;
  $effect(() => {
    const key = ids.join(",");
    const seq = ++metaSeq;
    meta = null;
    data = null;
    error = null;
    if (!key) return;
    api
      .timelineMeta(ids)
      .then((m) => {
        if (seq !== metaSeq) return;
        meta = m;
        fit();
      })
      .catch((e) => (error = message(e)));
  });

  // Fetch detections for the view, debounced; stale answers are dropped.
  let dataSeq = 0;
  $effect(() => {
    if (!meta || !ids.length) return;
    const { t0, t1 } = view;
    const bins = Math.max(10, Math.floor(plotW / 3));
    const seq = ++dataSeq;
    const timer = setTimeout(() => {
      loading = true;
      api
        .timelineData(ids, t0, t1, bins)
        .then((d) => {
          if (seq === dataSeq) data = d;
        })
        .catch((e) => (error = message(e)))
        .finally(() => {
          if (seq === dataSeq) loading = false;
        });
    }, 120);
    return () => clearTimeout(timer);
  });

  // ---------------------------------------------------------------- axis
  const STEPS = [
    1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400, 172800, 604800,
    1209600,
  ];
  const MONTH = 30.44 * 86400;

  function ticks(): { t: number; label: string; major: boolean }[] {
    const span = view.t1 - view.t0;
    const target = span / Math.max(2, plotW / 110);
    const out: { t: number; label: string; major: boolean }[] = [];
    if (target < 20 * 86400) {
      const step = STEPS.find((s) => s >= target) ?? 1209600;
      const first = Math.ceil(view.t0 / step) * step;
      for (let t = first; t <= view.t1; t += step) {
        const d = new Date(t * 1000);
        const midnight = d.getUTCHours() === 0 && d.getUTCMinutes() === 0 && d.getUTCSeconds() === 0;
        let label: string;
        if (step >= 86400 || midnight) label = fmt.date(t).replace(/ \d{4}$/, "");
        else if (step < 60) label = fmt.timeOfDay(t);
        else label = fmt.timeOfDay(t).slice(0, 5);
        out.push({ t, label, major: midnight });
      }
      return out;
    }
    // Calendar steps: months or years.
    const months = [1, 2, 3, 6, 12, 24, 60, 120].find((m) => m * MONTH >= target) ?? 120;
    const d0 = new Date(view.t0 * 1000);
    let y = d0.getUTCFullYear(), m = d0.getUTCMonth();
    m = Math.ceil(m / Math.min(months, 12)) * Math.min(months, 12);
    if (months >= 12) {
      y = Math.ceil((y + (d0.getUTCMonth() > 0 ? 1 : 0)) / (months / 12)) * (months / 12);
      m = 0;
    }
    for (let i = 0; i < 200; i++) {
      const t = Date.UTC(y, m, 1) / 1000;
      if (t > view.t1) break;
      if (t >= view.t0) {
        const date = new Date(t * 1000);
        const label = m === 0 ? String(y) : date.toLocaleString("en-GB", { month: "short", timeZone: "UTC" });
        out.push({ t, label, major: m === 0 });
      }
      m += months;
      y += Math.floor(m / 12);
      m %= 12;
    }
    return out;
  }

  // ---------------------------------------------------------------- draw
  function css(name: string) {
    return getComputedStyle(canvas).getPropertyValue(name).trim();
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(canvasH * dpr);
    canvas.style.height = `${canvasH}px`;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, canvasH);
    const text = css("--text"), text2 = css("--text-2"), text3 = css("--text-3");
    const grid = css("--grid"), accent = css("--accent-strong"), effort = css("--effort");
    g.font = `12px ${css("--font")}`;
    g.textBaseline = "middle";

    // Recording period(s) as a band under the axis.
    for (const r of meta?.recording ?? []) {
      const a = Math.max(GUTTER, x(r.start)), b = Math.min(GUTTER + plotW, x(r.end));
      if (b > a) {
        g.fillStyle = grid;
        g.fillRect(a, AXIS - 5, Math.max(1, b - a), 3);
      }
    }

    // Axis and grid lines.
    g.save();
    g.beginPath();
    g.rect(GUTTER, 0, plotW, canvasH);
    g.clip();
    for (const tk of ticks()) {
      const px = Math.round(x(tk.t)) + 0.5;
      g.strokeStyle = grid;
      g.globalAlpha = tk.major ? 1 : 0.55;
      g.beginPath();
      g.moveTo(px, AXIS);
      g.lineTo(px, canvasH);
      g.stroke();
      g.globalAlpha = 1;
      g.fillStyle = tk.major ? text2 : text3;
      g.fillText(tk.label, px + 4, 11);
    }
    g.restore();

    const laneIndex = new Map(lanes.map((l, i) => [l.key, i]));
    const top = (i: number) => AXIS + i * rowH;

    // Lane labels and effort.
    lanes.forEach((l, i) => {
      const y = top(i);
      if (i % 2 === 0) {
        g.fillStyle = "rgba(255,255,255,0.018)";
        g.fillRect(0, y, width, rowH);
      }
      const name = app.speciesName(l.species_tsn);
      g.fillStyle = l.n_detections ? text : text3;
      const label = l.call ? `${name} · ${l.call}` : name;
      g.fillText(ellipsis(g, label, GUTTER - 64), 12, y + rowH / 2);
      g.fillStyle = text3;
      g.textAlign = "right";
      g.fillText(l.n_detections ? fmt.compact(l.n_detections) : "none", GUTTER - 12, y + rowH / 2);
      g.textAlign = "left";
    });
    g.save();
    g.beginPath();
    g.rect(GUTTER, AXIS, plotW, canvasH - AXIS);
    g.clip();
    g.fillStyle = effort;
    for (const e of meta?.effort ?? []) {
      const i = laneIndex.get(e.lane);
      if (i == null) continue;
      const a = x(e.start), b = x(e.end);
      if (b < GUTTER || a > GUTTER + plotW) continue;
      g.fillRect(a, top(i) + 3, Math.max(1, b - a), rowH - 6);
    }

    // Detections.
    if (data) {
      const keyLane = data.keys.map((k) => laneIndex.get(k) ?? -1);
      g.fillStyle = accent;
      if (data.mode === "bins") {
        const bw = (data.t1 - data.t0) / data.n_bins;
        const max = new Array(lanes.length).fill(0);
        data.count.forEach((c, j) => {
          const li = keyLane[data!.bin_key[j]];
          if (li >= 0) max[li] = Math.max(max[li], c);
        });
        data.count.forEach((c, j) => {
          const li = keyLane[data!.bin_key[j]];
          if (li < 0) return;
          const t = data!.t0 + data!.bin[j] * bw;
          const a = x(t), b = x(t + bw);
          const w = b - a > 3 ? b - a - 1 : Math.max(1, b - a);
          const h = Math.max(2, (rowH - 6) * Math.sqrt(c / max[li]));
          g.fillRect(a, top(li) + rowH - 3 - h, w, h);
        });
      } else {
        const h = Math.max(6, rowH * 0.55);
        g.globalAlpha = 0.85;
        data.start.forEach((s, j) => {
          const li = keyLane[data!.event_key[j]];
          if (li < 0) return;
          const a = x(s), b = x(data!.end[j]);
          g.fillRect(a - (b - a < 2 ? 1 : 0), top(li) + (rowH - h) / 2, Math.max(2, b - a), h);
        });
        g.globalAlpha = 1;
      }
    }
    g.restore();

    // Gutter divider.
    g.strokeStyle = grid;
    g.beginPath();
    g.moveTo(GUTTER - 0.5, 0);
    g.lineTo(GUTTER - 0.5, canvasH);
    g.stroke();

    // Crosshair.
    if (hover && hover.x >= GUTTER) {
      g.strokeStyle = text2;
      g.globalAlpha = 0.5;
      g.beginPath();
      g.moveTo(Math.round(hover.x) + 0.5, AXIS);
      g.lineTo(Math.round(hover.x) + 0.5, canvasH);
      g.stroke();
      g.globalAlpha = 1;
    }
  }

  function ellipsis(g: CanvasRenderingContext2D, s: string, w: number) {
    if (g.measureText(s).width <= w) return s;
    while (s.length > 1 && g.measureText(s + "…").width > w) s = s.slice(0, -1);
    return s + "…";
  }

  $effect(() => {
    // Redraw on anything drawn above.
    meta; data; view; width; canvasH; hover; app.species;
    draw();
  });

  // ---------------------------------------------------------------- hover
  function describe(px: number, py: number): string | null {
    if (px < GUTTER || !lanes.length) return null;
    const li = Math.floor((py - AXIS) / rowH);
    const lane = lanes[li];
    const t = tAt(px);
    const head = lane
      ? `<div class="tip-title">${app.speciesName(lane.species_tsn)}${lane.call ? ` · ${lane.call}` : ""}</div>`
      : "";
    if (!lane || !data) return `${head}<div>${fmt.dateTime(t)} UTC</div>`;
    const k = data.keys.indexOf(lane.key);
    const onEffort = (meta?.effort ?? []).some((e) => e.lane === lane.key && e.start <= t && t <= e.end);
    const effortLine = `<div class="muted">${onEffort ? "Within analysis effort" : "Not analysed"}</div>`;
    if (data.mode === "bins") {
      const bw = (data.t1 - data.t0) / data.n_bins;
      const b = Math.floor((t - data.t0) / bw);
      let c = 0;
      for (let j = 0; j < data.bin.length; j++) if (data.bin[j] === b && data.bin_key[j] === k) c += data.count[j];
      const from = data.t0 + b * bw;
      return `${head}<div>${fmt.dateTime(from)} – ${fmt.dateTime(from + bw)} UTC</div>
        <div><b>${fmt.count(c)}</b> detection${c === 1 ? "" : "s"}</div>${effortLine}`;
    }
    // Nearest detection in this lane within 6 px.
    const tol = (6 / plotW) * (view.t1 - view.t0);
    let best = -1, dist = Infinity;
    for (let j = 0; j < data.start.length; j++) {
      if (data.event_key[j] !== k) continue;
      const d = t < data.start[j] ? data.start[j] - t : t > data.end[j] ? t - data.end[j] : 0;
      if (d < dist) (dist = d), (best = j);
    }
    if (best >= 0 && dist <= tol) {
      const s = data.start[best], e = data.end[best];
      const dur = e > s ? ` · ${(e - s).toFixed(e - s < 10 ? 2 : 0)} s` : "";
      return `${head}<div>${fmt.date(s)} ${fmt.timeOfDay(s, true)} UTC${dur}</div>`;
    }
    return `${head}<div>${fmt.dateTime(t)} UTC</div>${effortLine}`;
  }

  // ---------------------------------------------------------------- input
  let drag = $state<{ x: number; t0: number; t1: number } | null>(null);

  function pos(e: MouseEvent) {
    const r = canvas.getBoundingClientRect();
    return { px: e.clientX - r.left, py: e.clientY - r.top };
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const { px } = pos(e);
    if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      pan(((e.deltaX || e.deltaY) / plotW) * 0.8);
      return;
    }
    const factor = Math.exp(e.deltaY * 0.0015);
    zoom(factor, px >= GUTTER ? tAt(px) : undefined);
  }

  function onDown(e: MouseEvent) {
    if (e.button !== 0) return;
    drag = { x: e.clientX, t0: view.t0, t1: view.t1 };
    window.addEventListener("mousemove", onDrag);
    window.addEventListener("mouseup", onUp, { once: true });
  }

  function onDrag(e: MouseEvent) {
    if (!drag) return;
    const dt = ((e.clientX - drag.x) / plotW) * (drag.t1 - drag.t0);
    setView(drag.t0 - dt, drag.t1 - dt);
  }

  function onUp() {
    drag = null;
    window.removeEventListener("mousemove", onDrag);
  }

  function onMove(e: MouseEvent) {
    const { px, py } = pos(e);
    const html = describe(px, py);
    hover = html ? { x: px, y: py, html } : null;
  }

  function onKey(e: KeyboardEvent) {
    const k: Record<string, () => void> = {
      "+": () => zoom(0.6), "=": () => zoom(0.6), "-": () => zoom(1 / 0.6),
      ArrowLeft: () => pan(-0.2), ArrowRight: () => pan(0.2), "0": fit,
    };
    if (k[e.key]) {
      e.preventDefault();
      k[e.key]();
    }
  }

  onMount(() => {
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
    });
    ro.observe(wrap);
    return () => ro.disconnect();
  });

  const modeLabel = $derived.by(() => {
    if (!data) return "";
    if (data.mode === "events") return `${fmt.count(data.start.length)} detections in view`;
    return `Detections per ${fmt.duration((data.t1 - data.t0) / data.n_bins).replace(/^1 /, "")}`;
  });
</script>

<div class="timeline">
  <header>
    <div class="title">
      <strong>{title}</strong>
      <span class="muted">{fmt.dateTime(view.t0)} – {fmt.dateTime(view.t1)} UTC</span>
    </div>
    <div class="meta">
      {#if loading}<span class="spinner" aria-label="Loading"></span>{/if}
      <span class="mode">{modeLabel}</span>
      <span class="key"><i class="swatch effort"></i>Analysed</span>
      <span class="key"><i class="swatch det"></i>Detections</span>
    </div>
    <div class="tools">
      <button class="icon" title="Pan left (←)" onclick={() => pan(-0.3)}>‹</button>
      <button class="icon" title="Pan right (→)" onclick={() => pan(0.3)}>›</button>
      <button class="icon" title="Zoom out (−)" onclick={() => zoom(1 / 0.5)}>−</button>
      <button class="icon" title="Zoom in (+)" onclick={() => zoom(0.5)}>+</button>
      <button class="text" title="Show everything (0)" onclick={fit}>Fit</button>
    </div>
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="body" bind:this={wrap} tabindex="0" onkeydown={onKey} role="application" aria-label="Detection timeline. Drag to pan, scroll to zoom.">
    {#if error}
      <p class="empty">Couldn't load detections: {error}</p>
    {:else if meta && !lanes.length}
      <p class="empty">No detections or analysis effort recorded for this deployment.</p>
    {:else if !meta}
      <p class="empty"><span class="spinner"></span></p>
    {/if}
    <canvas
      bind:this={canvas}
      style:width="{width}px"
      onwheel={onWheel}
      onmousedown={onDown}
      onmousemove={onMove}
      onmouseleave={() => (hover = null)}
      ondblclick={(e) => zoom(0.5, tAt(pos(e).px))}
      class:dragging={drag != null}
    ></canvas>
    {#if hover}
      <div
        class="tip floating"
        style:left="{Math.min(hover.x + 14, width - 250)}px"
        style:top="{hover.y + 14}px"
      >
        {@html hover.html}
      </div>
    {/if}
  </div>
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 6px 10px 6px 14px;
    border-bottom: 1px solid var(--border);
    min-height: 40px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
  }
  .title strong {
    font-weight: 650;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 14px;
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-3);
  }
  .swatch {
    width: 12px;
    height: 10px;
    border-radius: 2px;
    display: inline-block;
  }
  .swatch.effort {
    background: var(--effort);
    outline: 1px solid var(--border);
  }
  .swatch.det {
    background: var(--accent-strong);
  }
  .tools {
    display: flex;
    gap: 4px;
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
  }
  .body:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  canvas {
    display: block;
    cursor: grab;
  }
  canvas.dragging {
    cursor: grabbing;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-3);
    margin: 0;
    pointer-events: none;
  }
  .floating {
    position: absolute;
    pointer-events: none;
    z-index: 2;
    width: max-content;
    max-width: 240px;
  }
  .muted {
    color: var(--text-3);
    font-size: 12px;
  }
</style>
