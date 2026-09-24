<script lang="ts">
  import { api, message, type Deployment, type DeploymentDetail } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import * as fmt from "../lib/format";

  let { d }: { d: Deployment } = $props();

  let detail = $state<DeploymentDetail | null>(null);
  let error = $state<string | null>(null);
  let openSets = $state(false);

  $effect(() => {
    const id = d.id;
    detail = null;
    error = null;
    openSets = false;
    api
      .deploymentDetail(id)
      .then((x) => {
        if (id === d.id) detail = x;
      })
      .catch((e) => (error = message(e)));
  });

  const species = $derived.by(() => {
    const out = new Map<number, Set<string>>();
    for (const s of detail?.detection_sets ?? [])
      for (const k of s.kinds) {
        if (!out.has(k.species_tsn)) out.set(k.species_tsn, new Set());
        if (k.call) out.get(k.species_tsn)!.add(k.call);
      }
    return [...out.entries()];
  });

  const methods = $derived.by(() => {
    const m = new Map<string, number>();
    for (const s of detail?.detection_sets ?? []) {
      const k = [s.software, s.method].filter(Boolean).join(" — ") || "Unspecified method";
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return [...m.entries()];
  });

  function download() {
    app.exportScope = "selected";
    app.showExport = true;
  }
</script>

<section class="card" aria-label="Deployment details">
  <header>
    <div>
      <div class="eyebrow"><i class="dot" style:background={app.colorOf(d)}></i>{d.project}{d.site ? ` · ${d.site}` : ""}</div>
      <h2>{d.deployment_id}</h2>
    </div>
    <button class="icon" title="Close (Esc)" aria-label="Close" onclick={() => (app.selectedId = null)}>✕</button>
  </header>

  <dl class="facts">
    <div><dt>Deployed</dt><dd>{fmt.dateTime(d.t_deploy)}</dd></div>
    <div><dt>Recovered</dt><dd>{d.t_recover ? fmt.dateTime(d.t_recover) : "—"}</dd></div>
    {#if d.t_recover && d.t_recover > d.t_deploy}
      <div><dt>Duration</dt><dd>{fmt.duration(d.t_recover - d.t_deploy)}</dd></div>
    {/if}
    <div><dt>Position</dt><dd>{fmt.latLon(d.lat, d.lon)}</dd></div>
    {#if d.depth_m != null}<div><dt>Depth</dt><dd>{Math.round(d.depth_m)} m</dd></div>{/if}
    <div><dt>Platform</dt><dd>{d.platform}</dd></div>
    <div><dt>Instrument</dt><dd>{d.instrument_type ?? "—"}{d.instrument_id ? ` (${d.instrument_id})` : ""}</dd></div>
    {#if detail}
      <div>
        <dt>Recording</dt>
        <dd>
          {detail.n_channels} ch
          {#if detail.sample_rates_khz.length} · {detail.sample_rates_khz.map((r) => `${r} kHz`).join(", ")}{/if}
          {#if detail.duty_cycled} · duty cycled{/if}
        </dd>
      </div>
    {/if}
    {#if d.region}<div><dt>Region</dt><dd>{d.region}</dd></div>{/if}
  </dl>

  {#if error}
    <p class="error">{error}</p>
  {:else if !detail}
    <p class="loading"><span class="spinner"></span></p>
  {:else}
    {#if detail.abstract_text || detail.objectives}
      <p class="abstract">{detail.abstract_text ?? detail.objectives}</p>
    {/if}

    <div class="block">
      <h3>Detections</h3>
      {#if !detail.detection_sets.length}
        <p class="muted">No detection data for this deployment.</p>
      {:else}
        <p class="stat">
          <strong>{fmt.count(detail.detection_sets.reduce((s, x) => s + x.n_detections, 0))}</strong> detections in
          {detail.detection_sets.length} {detail.detection_sets.length === 1 ? "set" : "sets"}
        </p>
        <ul class="species">
          {#each species as [tsn, calls] (tsn)}
            <li>
              <span>{app.speciesName(tsn)}</span>
              {#if app.speciesLatin(tsn)}<em>{app.speciesLatin(tsn)}</em>{/if}
              {#if calls.size}<span class="calls">{[...calls].join(", ")}</span>{/if}
            </li>
          {/each}
        </ul>
        {#each methods as [m, n] (m)}
          <p class="method">{m}{n > 1 ? ` (${n} sets)` : ""}</p>
        {/each}
        <button class="link" onclick={() => (openSets = !openSets)}>{openSets ? "Hide" : "Show"} detection sets</button>
        {#if openSets}
          <ul class="sets">
            {#each detail.detection_sets as s (s.doc_id)}
              <li>
                <span class="doc">{s.doc_id}</span>
                <span class="muted">{fmt.dateTime(s.t_start)} – {fmt.dateTime(s.t_end)} · {fmt.count(s.n_detections)}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </div>

    <div class="block">
      <h3>Data files</h3>
      {#if detail.files.length}
        <ul class="files">
          {#each detail.files as f (f.format + f.role)}
            <li><span>{fmt.count(f.n)} × {f.format}</span><span class="muted">{f.role.replace("_", " ")} · {fmt.bytes(f.bytes)}</span></li>
          {/each}
        </ul>
      {:else}
        <p class="muted">No raw data files registered{detail.audio_uri ? "; audio is described at" : "."}</p>
        {#if detail.audio_uri}<p class="uri">{detail.audio_uri}</p>{/if}
      {/if}
    </div>
  {/if}

  <footer>
    {#if d.n_detections}
      <button class="secondary" onclick={() => (app.timelineOpen = !app.timelineOpen)}>
        {app.timelineOpen ? "Hide timeline" : "Show timeline"}
      </button>
    {/if}
    <button class="primary" onclick={download}>Download…</button>
  </footer>
</section>

<style>
  .card {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 340px;
    max-height: calc(100% - 24px);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    backdrop-filter: blur(14px);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    overflow: hidden;
    z-index: 5;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
    padding: 14px 14px 8px 16px;
  }
  .eyebrow {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  h2 {
    margin: 2px 0 0;
    font-size: 17px;
    font-weight: 650;
    word-break: break-word;
  }
  .facts {
    margin: 0;
    padding: 4px 16px 10px;
    display: grid;
    gap: 5px;
    font-size: 13px;
    overflow: visible;
  }
  .facts div {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 8px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
    color: var(--text);
  }
  .abstract {
    margin: 0 16px 8px;
    font-size: 12.5px;
    color: var(--text-2);
    line-height: 1.5;
    max-height: 7.5em;
    overflow: auto;
  }
  .block {
    border-top: 1px solid var(--border);
    padding: 10px 16px 12px;
  }
  .card > .block:last-of-type {
    overflow-y: auto;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
    font-weight: 650;
  }
  .stat {
    margin: 0 0 6px;
    font-size: 13px;
  }
  .species {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
    display: grid;
    gap: 3px;
    font-size: 13px;
  }
  .species em {
    color: var(--text-3);
    margin-left: 6px;
    font-size: 12px;
  }
  .calls {
    color: var(--text-2);
    font-size: 12px;
    margin-left: 6px;
  }
  .calls::before {
    content: "· ";
  }
  .method {
    margin: 0 0 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .sets,
  .files {
    list-style: none;
    padding: 0;
    margin: 6px 0 0;
    display: grid;
    gap: 5px;
    font-size: 12px;
    max-height: 180px;
    overflow: auto;
  }
  .sets li,
  .files li {
    display: flex;
    flex-direction: column;
  }
  .doc {
    word-break: break-all;
  }
  .uri {
    font-size: 12px;
    word-break: break-all;
    color: var(--text-2);
    margin: 4px 0 0;
  }
  .muted {
    color: var(--text-3);
    font-size: 12px;
    margin: 0;
  }
  .error {
    color: var(--danger);
    padding: 0 16px;
    font-size: 13px;
  }
  .loading {
    text-align: center;
    padding: 12px;
    margin: 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 14px 14px;
    border-top: 1px solid var(--border);
    margin-top: auto;
  }
</style>
