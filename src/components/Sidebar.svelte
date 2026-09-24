<script lang="ts">
  import { app } from "../lib/state.svelte";
  import * as fmt from "../lib/format";

  const PAGE = 150;
  let shown = $state(PAGE);

  const list = $derived(app.filtered.slice(0, shown));
  const totalDetections = $derived(app.filtered.reduce((s, d) => s + d.n_detections, 0));
  const dataRange = $derived.by(() => {
    let a = Infinity, b = -Infinity;
    for (const d of app.deployments) {
      a = Math.min(a, d.t_deploy);
      b = Math.max(b, d.t_recover ?? d.t_deploy);
    }
    return Number.isFinite(a) ? { a, b } : null;
  });

  $effect(() => {
    app.filtered;
    shown = PAGE;
  });

  function toggleInstrument(k: string) {
    app.instruments = app.instruments.includes(k) ? app.instruments.filter((x) => x !== k) : [...app.instruments, k];
  }

  function areaText() {
    const a = app.area!;
    const lat = (v: number) => `${Math.abs(v).toFixed(1)}°${v >= 0 ? "N" : "S"}`;
    const lon = (v: number) => `${Math.abs(v).toFixed(1)}°${v >= 0 ? "E" : "W"}`;
    return `${lat(a.south)}–${lat(a.north)}, ${lon(a.west)}–${lon(a.east)}`;
  }

  function exportFiltered() {
    app.exportScope = "filtered";
    app.showExport = true;
  }
</script>

<aside>
  <div class="search">
    <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" /><path d="M13 13l4.5 4.5" /></svg>
    <input type="search" placeholder="Search deployment, project, site…" bind:value={app.text} />
  </div>

  <section class="filters">
    <div class="group">
      <div class="label">
        Time period
        {#if app.t0 != null || app.t1 != null}
          <button class="link" onclick={() => (app.t0 = app.t1 = null)}>Clear</button>
        {/if}
      </div>
      <div class="dates">
        <input
          type="date"
          aria-label="From"
          value={app.t0 != null ? fmt.isoDay(app.t0) : ""}
          min={dataRange ? fmt.isoDay(dataRange.a) : undefined}
          max={dataRange ? fmt.isoDay(dataRange.b) : undefined}
          onchange={(e) => (app.t0 = fmt.parseDay(e.currentTarget.value))}
        />
        <span>to</span>
        <input
          type="date"
          aria-label="To"
          value={app.t1 != null ? fmt.isoDay(app.t1 - 86400) : ""}
          min={dataRange ? fmt.isoDay(dataRange.a) : undefined}
          max={dataRange ? fmt.isoDay(dataRange.b) : undefined}
          onchange={(e) => {
            const t = fmt.parseDay(e.currentTarget.value);
            app.t1 = t == null ? null : t + 86400;
          }}
        />
      </div>
      {#if dataRange}
        <div class="hint">Data from {fmt.date(dataRange.a)} to {fmt.date(dataRange.b)}</div>
      {/if}
    </div>

    <div class="group">
      <div class="label">
        Instrument
        {#if app.instruments.length}
          <button class="link" onclick={() => (app.instruments = [])}>All</button>
        {/if}
      </div>
      <div class="chips">
        {#each app.instrumentTypes as [k, n] (k)}
          <button
            class="chip"
            class:on={app.instruments.includes(k)}
            aria-pressed={app.instruments.includes(k)}
            onclick={() => toggleInstrument(k)}
          >
            {k}<span class="n">{n}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="group">
      <div class="label">
        Area
        {#if app.area}<button class="link" onclick={() => (app.area = null)}>Clear</button>{/if}
      </div>
      {#if app.area}
        <div class="area">{areaText()}</div>
      {:else}
        <button class="secondary wide" class:active={app.drawingArea} onclick={() => (app.drawingArea = !app.drawingArea)}>
          {app.drawingArea ? "Click two corners on the globe…" : "Draw an area on the globe"}
        </button>
      {/if}
    </div>

    <label class="check">
      <input type="checkbox" bind:checked={app.onlyWithDetections} />
      Only deployments with detections
    </label>
  </section>

  <div class="summary">
    <div>
      <strong>{fmt.count(app.filtered.length)}</strong>
      {app.filtered.length === 1 ? "deployment" : "deployments"}
      {#if app.filtersActive}<span class="muted"> of {fmt.count(app.deployments.length)}</span>{/if}
      <div class="muted">{fmt.count(totalDetections)} detections</div>
    </div>
    <div class="actions">
      {#if app.filtersActive}<button class="link" onclick={() => app.clearFilters()}>Reset</button>{/if}
      <button class="primary small" disabled={!app.filtered.length} onclick={exportFiltered}>Download…</button>
    </div>
  </div>

  <ul class="list" role="listbox" aria-label="Deployments">
    {#each list as d (d.id)}
      <li>
        <button
          role="option"
          aria-selected={d.id === app.selectedId}
          class:selected={d.id === app.selectedId}
          onclick={() => (app.selectedId = d.id === app.selectedId ? null : d.id)}
        >
          <i class="dot" style:background={app.colorOf(d)} class:hollow={!d.n_detections}></i>
          <span class="main">
            <span class="name">{d.deployment_id}</span>
            <span class="sub">{[d.project, d.site].filter(Boolean).join(" · ")} · {fmt.date(d.t_deploy)}</span>
          </span>
          {#if d.n_detections}<span class="badge">{fmt.compact(d.n_detections)}</span>{/if}
        </button>
      </li>
    {/each}
    {#if app.filtered.length > shown}
      <li><button class="more" onclick={() => (shown += PAGE)}>Show {Math.min(PAGE, app.filtered.length - shown)} more</button></li>
    {/if}
    {#if !app.filtered.length && !app.loading}
      <li class="none">No deployments match these filters.</li>
    {/if}
  </ul>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel-solid);
    border-right: 1px solid var(--border);
  }
  .search {
    position: relative;
    margin: 12px 12px 4px;
  }
  .search svg {
    position: absolute;
    left: 10px;
    top: 50%;
    width: 16px;
    height: 16px;
    transform: translateY(-50%);
    fill: none;
    stroke: var(--text-3);
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .search input {
    width: 100%;
    padding-left: 34px;
  }
  .filters {
    padding: 8px 12px 4px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 14px;
  }
  .label {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
    margin-bottom: 6px;
  }
  .dates {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 6px;
    align-items: center;
    color: var(--text-3);
    font-size: 12px;
  }
  .dates input {
    min-width: 0;
    padding: 6px 6px;
    font-size: 12px;
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 5px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-2);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 12px;
  }
  .chip:hover {
    border-color: var(--text-3);
    color: var(--text);
  }
  .chip.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
  }
  .chip .n {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .area {
    font-size: 13px;
    color: var(--text);
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    border-radius: 8px;
    padding: 6px 10px;
  }
  .wide {
    width: 100%;
  }
  .secondary.active {
    border-color: var(--accent);
    color: var(--text);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    color: var(--text-2);
    cursor: pointer;
  }
  .summary {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }
  .summary .actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .muted {
    color: var(--text-3);
    font-size: 12px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 4px 6px 12px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .list li > button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    padding: 7px 8px;
    border-radius: 8px;
    color: var(--text);
  }
  .list li > button:hover {
    background: var(--hover);
  }
  .list li > button.selected {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent-strong);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
  }
  .dot.hollow {
    opacity: 0.45;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .name {
    font-size: 13px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    font-size: 11px;
    color: var(--text-2);
    background: var(--hover);
    border-radius: 999px;
    padding: 1px 7px;
    font-variant-numeric: tabular-nums;
  }
  .more {
    justify-content: center;
    color: var(--accent) !important;
    font-size: 13px;
  }
  .none {
    color: var(--text-3);
    font-size: 13px;
    padding: 20px 8px;
    text-align: center;
  }
</style>
