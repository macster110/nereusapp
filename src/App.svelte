<script lang="ts">
  import Globe from "./components/Globe.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import DetailPanel from "./components/DetailPanel.svelte";
  import Timeline from "./components/Timeline.svelte";
  import ExportDialog from "./components/ExportDialog.svelte";
  import ConnectDialog from "./components/ConnectDialog.svelte";
  import { app, type Basemap, type ColorBy } from "./lib/state.svelte";
  import * as fmt from "./lib/format";

  let globe: Globe | undefined = $state();
  let drawerH = $state(Number(localStorage.getItem("nereus.timelineHeight")) || 280);
  let timelineWants = $state(0);
  // The user's height is a maximum: a timeline with few lanes takes less.
  const drawerShown = $derived(timelineWants ? Math.max(130, Math.min(drawerH, timelineWants)) : drawerH);

  const timelineIds = $derived(app.selected ? [app.selected.id] : []);
  const showTimeline = $derived(app.selected != null && app.selected.n_detection_sets > 0 && app.timelineOpen);

  function resize(e: PointerEvent) {
    const startY = e.clientY, startH = drawerShown;
    const move = (m: PointerEvent) => {
      drawerH = Math.min(Math.max(140, startH - (m.clientY - startY)), window.innerHeight - 220);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      localStorage.setItem("nereus.timelineHeight", String(Math.round(drawerH)));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.showExport && !app.showConnect && !app.drawingArea) app.selectedId = null;
  }

  const BASEMAPS: { v: Basemap; label: string }[] = [
    { v: "dark", label: "Dark" },
    { v: "satellite", label: "Satellite" },
    { v: "ocean", label: "Ocean" },
  ];
  const COLOR_BY: { v: ColorBy; label: string }[] = [
    { v: "project", label: "Project" },
    { v: "instrument", label: "Instrument" },
    { v: "platform", label: "Platform" },
  ];
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <header class="top">
    <div class="brand"><img src="/icon.png" alt="" width="22" height="22" />Nereus</div>
    {#if app.info}
      <button class="conn" title="Change database" onclick={() => (app.showConnect = true)}>
        <i class="live"></i>{app.info.label}
        <span class="muted">· {fmt.count(app.info.n_deployments)} deployments · ~{fmt.compact(app.info.n_detections_estimate)} detections</span>
      </button>
      <button class="link" onclick={() => app.disconnect()}>Disconnect</button>
    {/if}
  </header>

  <div class="body">
    <Sidebar />
    <main class="stage">
      <div class="globe-area">
        <Globe bind:this={globe} />

        <div class="controls">
          <div class="segmented" role="radiogroup" aria-label="Basemap">
            {#each BASEMAPS as b (b.v)}
              <button role="radio" aria-checked={app.basemap === b.v} class:on={app.basemap === b.v} onclick={() => (app.basemap = b.v)}>{b.label}</button>
            {/each}
          </div>
          <button class="glass" title="Show all data" onclick={() => globe?.resetView()}>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 7V3h4M13 3h4v4M17 13v4h-4M7 17H3v-4" /></svg>
          </button>
        </div>

        {#if app.deployments.length}
          <div class="legend">
            <label>
              Colour by
              <select bind:value={app.colorBy}>
                {#each COLOR_BY as c (c.v)}<option value={c.v}>{c.label}</option>{/each}
              </select>
            </label>
            <ul>
              {#each app.legend.items as it (it.label)}
                <li><i style:background={it.color}></i><span>{it.label}</span><span class="n">{fmt.count(it.n)}</span></li>
              {/each}
            </ul>
            <p class="faint">Faded points: no detections</p>
          </div>
        {/if}

        {#if app.selected}
          {#key app.selected.id}<DetailPanel d={app.selected} />{/key}
        {/if}

        {#if app.loading}
          <div class="loading"><span class="spinner"></span> Loading deployments…</div>
        {:else if app.error}
          <div class="loading error">{app.error}</div>
        {/if}
      </div>

      {#if showTimeline && app.selected}
        <section class="drawer" style:height="{drawerShown}px">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="grip" onpointerdown={resize} title="Drag to resize"></div>
          <Timeline ids={timelineIds} title={`Detections · ${app.selected.deployment_id}`} bind:preferredHeight={timelineWants} />
          <button class="icon collapse" title="Hide timeline" aria-label="Hide timeline" onclick={() => (app.timelineOpen = false)}>⌄</button>
        </section>
      {/if}
    </main>
  </div>

  {#if app.showExport}<ExportDialog />{/if}
  {#if app.showConnect}<ConnectDialog />{/if}
</div>

<style>
  .app {
    height: 100vh;
    display: grid;
    grid-template-rows: 44px 1fr;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 14px;
    background: var(--panel-solid);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    letter-spacing: 0.02em;
    font-size: 15px;
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--hover);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
    padding: 4px 12px;
    font-size: 12.5px;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .conn:hover {
    border-color: var(--border-strong);
  }
  .live {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-2);
    box-shadow: 0 0 0 3px rgba(126, 224, 195, 0.18);
  }
  .muted {
    color: var(--text-3);
  }
  .body {
    display: grid;
    grid-template-columns: 320px 1fr;
    min-height: 0;
  }
  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .globe-area {
    position: relative;
    flex: 1;
    min-height: 0;
    background: #000;
  }
  .controls {
    position: absolute;
    top: 12px;
    left: 12px;
    display: flex;
    gap: 8px;
    z-index: 4;
  }
  .segmented {
    display: flex;
    background: var(--panel);
    backdrop-filter: blur(12px);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 3px;
  }
  .segmented button {
    background: none;
    border: 0;
    color: var(--text-2);
    padding: 4px 11px;
    border-radius: 7px;
    font-size: 12.5px;
  }
  .segmented button.on {
    background: var(--accent-soft);
    color: var(--text);
  }
  .glass {
    display: grid;
    place-items: center;
    width: 34px;
    background: var(--panel);
    backdrop-filter: blur(12px);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text-2);
  }
  .glass svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
  }
  .legend {
    position: absolute;
    left: 12px;
    bottom: 12px;
    background: var(--panel);
    backdrop-filter: blur(12px);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 10px 12px;
    font-size: 12px;
    max-width: 240px;
    z-index: 4;
  }
  .legend label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    color: var(--text-3);
    margin-bottom: 6px;
  }
  .legend select {
    padding: 2px 6px;
    font-size: 12px;
  }
  .legend ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .legend li i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex: none;
  }
  .legend li span:first-of-type {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .legend .n {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .faint {
    margin: 6px 0 0;
    color: var(--text-3);
    font-size: 11px;
  }
  .loading {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 16px;
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .loading.error {
    color: var(--danger);
  }
  .drawer {
    position: relative;
    background: var(--panel-solid);
    border-top: 1px solid var(--border);
    flex: none;
  }
  .grip {
    position: absolute;
    top: -4px;
    left: 0;
    right: 0;
    height: 8px;
    cursor: ns-resize;
    z-index: 3;
  }
  .grip::after {
    content: "";
    position: absolute;
    left: 50%;
    top: 2px;
    width: 40px;
    height: 4px;
    margin-left: -20px;
    border-radius: 2px;
    background: var(--border-strong);
  }
  .collapse {
    position: absolute;
    top: 6px;
    right: 8px;
    display: none;
  }
</style>
