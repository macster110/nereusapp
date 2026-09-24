<script lang="ts">
  // Deployments too close together on the globe to click one at a time.
  // A few fan out around the spot with leader lines, keeping their direction
  // from it; many (e.g. a site revisited for years) open as a list instead.
  import { onMount } from "svelte";
  import type { Deployment } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import * as fmt from "../lib/format";

  interface Props {
    /** The clicked spot, in pixels within the globe area. */
    x: number;
    y: number;
    width: number;
    height: number;
    items: Deployment[];
    /** Screen offset of each item from the spot (same order as items). */
    offsets: { dx: number; dy: number }[];
    onpick: (id: number) => void;
    onclose: () => void;
  }
  let { x, y, width, height, items, offsets, onpick, onclose }: Props = $props();

  const FAN_MAX = 12;
  const fan = $derived(items.length <= FAN_MAX);
  let open = $state(false);
  let panel: HTMLDivElement | undefined = $state();
  let query = $state("");

  // ------------------------------------------------------------ fan layout
  // Callout columns: chips stack in evenly spaced rows right and left of the
  // spot, so labels never overlap; leader lines fan out to them. Going
  // clockwise from the top, the first half go right and the rest left, so
  // each chip sits roughly in its direction from the spot (or, for points
  // at the same position, in date order).
  const ROW = 36;
  const GAP = 96;

  const spokes = $derived.by(() => {
    const n = items.length;
    const spread = Math.max(...offsets.map((o) => Math.hypot(o.dx, o.dy)));
    const clockwiseFromTop = (i: number) => {
      const a = Math.atan2(offsets[i].dy, offsets[i].dx) + Math.PI / 2;
      return a < 0 ? a + 2 * Math.PI : a;
    };
    const order = items.map((d, i) => ({ d, key: spread < 3 ? d.t_deploy : clockwiseFromTop(i) }));
    order.sort((p, q) => p.key - q.key);
    const nRight = Math.ceil(n / 2);
    const right = order.slice(0, nRight);
    const left = order.slice(nRight).reverse(); // continues clockwise: bottom to top
    const place = (col: typeof order, side: 1 | -1) =>
      col.map((o, i) => {
        const top = Math.min(Math.max(y - ((col.length - 1) * ROW) / 2, 20), height - 20 - (col.length - 1) * ROW);
        const px = Math.min(Math.max(x + side * GAP, 16), width - 16);
        const py = top + i * ROW;
        const dx = px - x, dy = py - y;
        return { d: o.d, dx, dy, len: Math.hypot(dx, dy), angle: Math.atan2(dy, dx), left: side < 0 };
      });
    return [...place(right, 1), ...place(left, -1)].map((s, k) => ({ ...s, k }));
  });

  // ------------------------------------------------------------ list layout
  const PANEL_W = 330;
  const sorted = $derived([...items].sort((a, b) => a.t_deploy - b.t_deploy));
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sorted;
    return sorted.filter((d) =>
      `${d.deployment_id} ${d.site ?? ""} ${d.project} ${fmt.date(d.t_deploy)}`.toLowerCase().includes(q),
    );
  });
  const span = $derived.by(() => {
    let a = Infinity, b = -Infinity;
    for (const d of items) {
      a = Math.min(a, d.t_deploy);
      b = Math.max(b, d.t_recover ?? d.t_deploy);
    }
    return { a, b: b > a ? b : a + 1 };
  });
  const places = $derived([...new Set(items.map((d) => [d.project, d.site].filter(Boolean).join(" · ")))]);
  const panelLeft = $derived(x + 28 + PANEL_W < width - 8 ? x + 28 : Math.max(8, x - 28 - PANEL_W));
  const panelTop = $derived(Math.min(Math.max(8, y - 60), Math.max(8, height - 420)));
  const originX = $derived(x - panelLeft);
  const originY = $derived(y - panelTop);

  function bar(d: Deployment) {
    const w = span.b - span.a;
    const from = ((d.t_deploy - span.a) / w) * 100;
    const to = (((d.t_recover ?? d.t_deploy) - span.a) / w) * 100;
    return `left:${from}%;width:${Math.max(2, to - from)}%`;
  }

  onMount(() => {
    requestAnimationFrame(() => (open = true));
    // Anything outside the picker closes it (including starting to rotate the globe).
    const down = (e: PointerEvent) => {
      const t = e.target as HTMLElement;
      if (!t.closest?.(".cluster-ui")) onclose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopImmediatePropagation();
        onclose();
      }
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
    };
  });
</script>

<div class="cluster" class:open aria-live="polite">
  <div class="spot" style:left="{x}px" style:top="{y}px">
    <span class="count">{items.length}</span>
  </div>

  {#if fan}
    {#each spokes as s (s.d.id)}
      <div
        class="leader"
        style:left="{x}px"
        style:top="{y}px"
        style:width="{s.len}px"
        style:transform="rotate({s.angle}rad) scaleX({open ? 1 : 0})"
        style:transition-delay="{s.k * 22}ms"
      ></div>
      <button
        class="cluster-ui chip"
        class:left={s.left}
        class:selected={s.d.id === app.selectedId}
        style:left="{x}px"
        style:top="{y}px"
        style:transform="translate({open ? s.dx : 0}px, {open ? s.dy : 0}px) scale({open ? 1 : 0.4})"
        style:transition-delay="{s.k * 22}ms"
        onclick={() => onpick(s.d.id)}
        title="{s.d.deployment_id}: {fmt.date(s.d.t_deploy)} – {fmt.date(s.d.t_recover)}"
      >
        <i class="dot" class:faded={!s.d.n_detections} style:background={app.colorOf(s.d)}></i>
        <span class="lbl">
          <b>{s.d.deployment_id}</b>
          <small>{fmt.date(s.d.t_deploy)}{s.d.n_detections ? ` · ${fmt.compact(s.d.n_detections)}` : ""}</small>
        </span>
      </button>
    {/each}
  {:else}
    <div
      class="cluster-ui panel"
      bind:this={panel}
      style:left="{panelLeft}px"
      style:top="{panelTop}px"
      style:width="{PANEL_W}px"
      style:transform-origin="{originX}px {originY}px"
      role="dialog"
      aria-label="{items.length} deployments at this spot"
    >
      <header>
        <div>
          <strong>{items.length} deployments here</strong>
          <small>{places.slice(0, 3).join(", ")}{places.length > 3 ? ` +${places.length - 3}` : ""}</small>
          <small>{fmt.date(span.a)} – {fmt.date(span.b)}</small>
        </div>
        <button class="icon" aria-label="Close" onclick={onclose}>✕</button>
      </header>
      {#if items.length > 15}
        <!-- svelte-ignore a11y_autofocus -->
        <input type="search" placeholder="Filter…" bind:value={query} autofocus />
      {/if}
      <ul>
        {#each shown as d (d.id)}
          <li>
            <button class:selected={d.id === app.selectedId} onclick={() => onpick(d.id)}>
              <i class="dot" class:faded={!d.n_detections} style:background={app.colorOf(d)}></i>
              <span class="lbl">
                <b>{d.deployment_id}</b>
                <small>{fmt.date(d.t_deploy)} – {fmt.date(d.t_recover)}</small>
              </span>
              <span class="span" aria-hidden="true"><i style={bar(d)}></i></span>
              <span class="n">{d.n_detections ? fmt.compact(d.n_detections) : "—"}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .cluster {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 6;
  }
  .spot {
    position: absolute;
    width: 26px;
    height: 26px;
    margin: -13px 0 0 -13px;
    border-radius: 50%;
    border: 2px solid var(--accent-2);
    background: rgba(7, 11, 20, 0.75);
    display: grid;
    place-items: center;
    transform: scale(0.3);
    opacity: 0;
    transition: transform 0.25s ease-out, opacity 0.2s;
    box-shadow: 0 0 0 6px rgba(126, 224, 195, 0.12);
  }
  .open .spot {
    transform: scale(1);
    opacity: 1;
  }
  .count {
    font-size: 11px;
    font-weight: 700;
    color: var(--text);
  }
  .leader {
    position: absolute;
    height: 1.5px;
    margin-top: -0.75px;
    background: linear-gradient(90deg, rgba(126, 224, 195, 0.9), rgba(126, 224, 195, 0.35));
    transform-origin: 0 50%;
    transition: transform 0.38s cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }
  .chip {
    position: absolute;
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 10px 4px 5px;
    margin: -14px 0 0 -9px;
    background: var(--panel);
    backdrop-filter: blur(10px);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    color: var(--text);
    white-space: nowrap;
    opacity: 0;
    transition:
      transform 0.38s cubic-bezier(0.2, 0.9, 0.3, 1.15),
      opacity 0.25s,
      border-color 0.15s,
      background 0.15s;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  }
  /* Chips left of the spot grow leftwards, so the dot sits at the spoke's end. */
  .chip.left {
    flex-direction: row-reverse;
    padding: 4px 5px 4px 10px;
    translate: calc(-100% + 18px) 0;
  }
  .chip.left .lbl {
    text-align: right;
  }
  .open .chip {
    opacity: 1;
  }
  .chip:hover,
  .chip:focus-visible {
    border-color: var(--accent);
    background: var(--panel-solid);
    z-index: 2;
  }
  .chip.selected {
    border-color: #fff;
  }
  .dot {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    flex: none;
    box-shadow: 0 0 0 2px rgba(7, 11, 20, 0.9);
  }
  .dot.faded {
    opacity: 0.5;
  }
  .lbl {
    display: flex;
    flex-direction: column;
    line-height: 1.2;
    text-align: left;
  }
  .lbl b {
    font-size: 12.5px;
    font-weight: 600;
  }
  .lbl small {
    font-size: 11px;
    color: var(--text-3);
  }

  .panel {
    position: absolute;
    pointer-events: auto;
    display: flex;
    flex-direction: column;
    max-height: 400px;
    background: var(--panel);
    backdrop-filter: blur(14px);
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    box-shadow: var(--shadow);
    transform: scale(0.6);
    opacity: 0;
    transition: transform 0.3s cubic-bezier(0.2, 0.9, 0.3, 1.1), opacity 0.2s;
  }
  .open .panel {
    transform: scale(1);
    opacity: 1;
  }
  .panel header {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 12px 12px 8px 14px;
  }
  .panel header div {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .panel header small {
    color: var(--text-3);
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .panel input {
    margin: 0 12px 6px;
    padding: 5px 9px;
    font-size: 12.5px;
  }
  .panel ul {
    list-style: none;
    margin: 0;
    padding: 2px 6px 8px;
    overflow-y: auto;
  }
  .panel li button {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    background: none;
    border: 0;
    border-radius: 8px;
    padding: 6px 8px;
    color: var(--text);
    text-align: left;
  }
  .panel li button:hover {
    background: var(--hover);
  }
  .panel li button.selected {
    background: var(--accent-soft);
  }
  .panel .dot {
    width: 10px;
    height: 10px;
    box-shadow: none;
  }
  .panel .lbl {
    flex: 1;
    min-width: 0;
  }
  .panel .lbl b {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Where this deployment falls in the time covered at this spot. */
  .span {
    position: relative;
    width: 64px;
    height: 6px;
    border-radius: 3px;
    background: var(--hover);
    flex: none;
  }
  .span i {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 3px;
    background: var(--accent);
  }
  .n {
    width: 36px;
    text-align: right;
    font-size: 11.5px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
</style>
