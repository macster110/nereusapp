<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { downloadDir } from "@tauri-apps/api/path";
  import { api, message, type ExportPreview, type ExportResult, type Progress } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import * as fmt from "../lib/format";

  const PREFS = "nereus.export";
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(PREFS) ?? "{}");
    } catch {
      return {};
    }
  })();

  let scope = $state<"selected" | "filtered">(app.selected ? app.exportScope : "filtered");
  let useTime = $state(app.t0 != null || app.t1 != null);
  let matlab = $state(saved.matlab ?? true);
  let r = $state(saved.r ?? false);
  let tethys = $state(saved.tethys ?? false);
  let raw = $state(false);
  let outDir = $state<string>(saved.outDir ?? "");
  let name = $state("");

  let preview = $state<ExportPreview | null>(null);
  let previewError = $state<string | null>(null);
  let running = $state(false);
  let progress = $state<Progress | null>(null);
  let result = $state<ExportResult | null>(null);
  let error = $state<string | null>(null);

  const ids = $derived(scope === "selected" && app.selected ? [app.selected.id] : app.filtered.map((d) => d.id));
  const t0 = $derived(useTime ? app.t0 : null);
  const t1 = $derived(useTime ? app.t1 : null);
  const anyFormat = $derived(matlab || r || tethys || raw);
  const canRun = $derived(ids.length > 0 && anyFormat && outDir !== "" && !running);

  function defaultName() {
    const d = new Date();
    const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
    const base = scope === "selected" && app.selected ? app.selected.deployment_id : `nereus_${ids.length}_deployments`;
    return `${base}_${stamp}`.replace(/[^\w.-]+/g, "_");
  }

  $effect(() => {
    scope;
    name = defaultName();
  });

  onMount(async () => {
    if (!outDir) outDir = await downloadDir().catch(() => "");
  });

  // Counts for what would be written, refreshed as the choices change.
  $effect(() => {
    const sel = { deployment_ids: ids, t0, t1 };
    preview = null;
    previewError = null;
    if (!ids.length) return;
    const timer = setTimeout(() => {
      api
        .exportPreview(sel)
        .then((p) => (preview = p))
        .catch((e) => (previewError = message(e)));
    }, 150);
    return () => clearTimeout(timer);
  });

  async function browse() {
    const dir = await open({ directory: true, defaultPath: outDir || undefined, title: "Save downloads in" });
    if (typeof dir === "string") outDir = dir;
  }

  async function run() {
    localStorage.setItem(PREFS, JSON.stringify({ matlab, r, tethys, outDir }));
    running = true;
    error = null;
    result = null;
    progress = { stage: "Starting", done: 0, total: 0, detail: "" };
    const unlisten = await api.onExportProgress((p) => (progress = p));
    try {
      result = await api.startExport({ deployment_ids: ids, t0, t1, matlab, r, tethys, raw, out_dir: outDir, name });
    } catch (e) {
      const m = message(e);
      error = m === "cancelled" ? "Download cancelled." : m;
    } finally {
      unlisten();
      running = false;
      progress = null;
    }
  }

  function close() {
    if (running) return;
    app.showExport = false;
  }

  const pct = $derived(progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : null);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="export-title">
    <header>
      <h2 id="export-title">Download data</h2>
      <button class="icon" aria-label="Close" onclick={close} disabled={running}>✕</button>
    </header>

    {#if result}
      <div class="done">
        <p class="ok">Download complete{result.n_detections ? ` · ${fmt.count(result.n_detections)} detections` : ""}</p>
        <ul class="written">
          {#each result.written as p (p)}
            <li>
              <span>{p.split(/[\\/]/).pop()}</span>
              <button class="link" onclick={() => revealItemInDir(p)}>Show in folder</button>
            </li>
          {/each}
        </ul>
        {#if result.warnings.length}
          <details class="warnings">
            <summary>{result.warnings.length} file{result.warnings.length === 1 ? "" : "s"} could not be fetched</summary>
            <ul>{#each result.warnings as w}<li>{w}</li>{/each}</ul>
          </details>
        {/if}
      </div>
      <footer>
        <button class="secondary" onclick={() => (result = null)}>Download more</button>
        <button class="primary" onclick={close}>Done</button>
      </footer>
    {:else}
      <div class="content">
        <fieldset>
          <legend>What</legend>
          {#if app.selected}
            <label class="radio">
              <input type="radio" bind:group={scope} value="selected" disabled={running} />
              <span>Selected deployment <strong>{app.selected.deployment_id}</strong></span>
            </label>
          {/if}
          <label class="radio">
            <input type="radio" bind:group={scope} value="filtered" disabled={running} />
            <span>
              {app.filtersActive ? "All deployments matching the filters" : "All deployments"}
              <span class="muted">({fmt.count(app.filtered.length)})</span>
            </span>
          </label>
          <label class="check">
            <input type="checkbox" bind:checked={useTime} disabled={running || (app.t0 == null && app.t1 == null)} />
            <span>
              Only detections in the filter's time period
              {#if app.t0 != null || app.t1 != null}
                <span class="muted">({fmt.date(app.t0)} – {app.t1 != null ? fmt.date(app.t1 - 86400) : "…"})</span>
              {:else}
                <span class="muted">(set a time period in the sidebar)</span>
              {/if}
            </span>
          </label>
        </fieldset>

        <fieldset>
          <legend>Formats</legend>
          <label class="format">
            <input type="checkbox" bind:checked={matlab} disabled={running} />
            <span>
              <b><strong>MATLAB</strong> <code>.mat</code></b>
              <small>A <code>deployments</code> struct array; each has its <code>detections</code> and <code>effort</code> as column vectors. Times are datenums (UTC).</small>
            </span>
          </label>
          <label class="format">
            <input type="checkbox" bind:checked={r} disabled={running} />
            <span>
              <b><strong>R</strong> <code>.RData</code></b>
              <small><code>deployments</code>, <code>effort</code> and <code>detections</code> data frames; <code>load()</code> them. Times are POSIXct (UTC).</small>
            </span>
          </label>
          <label class="format">
            <input type="checkbox" bind:checked={tethys} disabled={running} />
            <span>
              <b><strong>Tethys XML</strong> <code>.zip</code></b>
              <small>Deployment and Detections documents, valid against the Tethys schema.{#if preview}{" "}{fmt.count(preview.n_documents)} documents.{/if}</small>
            </span>
          </label>
          <label class="format" class:disabled={preview != null && preview.n_files === 0}>
            <input type="checkbox" bind:checked={raw} disabled={running || (preview != null && preview.n_files === 0)} />
            <span>
              <b><strong>Raw data files</strong></b>
              <small>
                {#if !preview}
                  Audio, PAMGuard binaries and other files registered for these deployments.
                {:else if preview.n_files === 0}
                  No raw files are registered for this selection.
                {:else}
                  {fmt.count(preview.n_files)} files · {fmt.bytes(preview.file_bytes)}
                  {#if preview.n_files_unsupported} · {preview.n_files_unsupported} in cloud storage this version can't fetch yet{/if}
                {/if}
              </small>
            </span>
          </label>
        </fieldset>

        <fieldset>
          <legend>Save to</legend>
          <div class="dest">
            <input type="text" bind:value={outDir} placeholder="Choose a folder" disabled={running} />
            <button class="secondary" onclick={browse} disabled={running}>Browse…</button>
          </div>
          <label class="name">
            <span>File name</span>
            <input type="text" bind:value={name} disabled={running} spellcheck="false" />
          </label>
        </fieldset>

        {#if previewError}<p class="error">{previewError}</p>{/if}
        {#if error}<p class="error">{error}</p>{/if}
      </div>

      <footer>
        {#if running && progress}
          <div class="progress">
            <div class="bar"><div class:indeterminate={pct == null} style:width={pct == null ? "30%" : `${pct}%`}></div></div>
            <div class="stage">
              {progress.stage}{#if pct != null && progress.stage.startsWith("Reading")} · {fmt.count(progress.done)} of {fmt.count(progress.total)}{/if}
              {#if progress.detail}<span class="muted"> {progress.detail}</span>{/if}
            </div>
          </div>
          <button class="secondary" onclick={() => api.cancelExport()}>Cancel</button>
        {:else}
          <div class="summary">
            {#if preview}
              {fmt.count(preview.n_deployments)} deployment{preview.n_deployments === 1 ? "" : "s"} ·
              {fmt.count(preview.n_detections)} detections
            {:else if ids.length}
              <span class="spinner"></span>
            {/if}
          </div>
          <button class="secondary" onclick={close}>Cancel</button>
          <button class="primary" disabled={!canRun} onclick={run}>Download</button>
        {/if}
      </footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(2, 5, 12, 0.6);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--panel-solid);
    border: 1px solid var(--border);
    border-radius: 16px;
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 18px 6px 20px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
  }
  .content {
    overflow-y: auto;
    padding: 4px 20px 8px;
  }
  fieldset {
    border: 0;
    margin: 0 0 14px;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  legend {
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
    margin-bottom: 6px;
    padding: 0;
  }
  label {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    font-size: 13.5px;
    cursor: pointer;
  }
  label input[type="radio"],
  label input[type="checkbox"] {
    margin-top: 3px;
  }
  .format {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
  }
  .format:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .format.disabled {
    opacity: 0.55;
    cursor: default;
  }
  .format > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .format b {
    font-weight: inherit;
  }
  small {
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.45;
  }
  code {
    font-family: var(--mono);
    font-size: 0.92em;
    color: var(--text-2);
  }
  .dest {
    display: flex;
    gap: 8px;
  }
  .dest input {
    flex: 1;
    min-width: 0;
  }
  .name {
    align-items: center;
  }
  .name span {
    color: var(--text-2);
    white-space: nowrap;
  }
  .name input {
    flex: 1;
  }
  .muted {
    color: var(--text-3);
  }
  .error {
    color: var(--danger);
    font-size: 13px;
    margin: 0 0 8px;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px 16px;
    border-top: 1px solid var(--border);
  }
  .summary {
    flex: 1;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .progress {
    flex: 1;
    min-width: 0;
  }
  .bar {
    height: 6px;
    background: var(--hover);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent-strong);
    border-radius: 3px;
    transition: width 0.2s;
  }
  .bar div.indeterminate {
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from { transform: translateX(-100%); }
    to { transform: translateX(340%); }
  }
  .stage {
    font-size: 12px;
    color: var(--text-2);
    margin-top: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .done {
    padding: 4px 20px 12px;
  }
  .ok {
    font-weight: 600;
    color: var(--accent-2);
  }
  .written {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 6px;
    font-size: 13px;
  }
  .written li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    word-break: break-all;
  }
  .warnings {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-2);
  }
</style>
