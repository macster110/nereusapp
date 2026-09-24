<script lang="ts">
  import { onMount } from "svelte";
  import Globe, { type GlobeInstance } from "globe.gl";
  import { app, type Area } from "../lib/state.svelte";
  import type { Deployment, Track } from "../lib/api";
  import * as fmt from "../lib/format";

  let el: HTMLDivElement;
  let globe: GlobeInstance | undefined = $state();
  let firstCorner = $state<{ lat: number; lng: number } | null>(null);
  /** The rectangle following the pointer after the first corner. */
  let rubberBand = $state<Area | null>(null);

  const TEXTURE = "/textures/earth-blue-marble.jpg";
  let darkTexture: Promise<string> | null = null;
  let altitude = $state(2.2);

  /** The satellite image dimmed and desaturated, so coloured points stand out. */
  function darkened(): Promise<string> {
    darkTexture ??= new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const c = document.createElement("canvas");
        c.width = img.width;
        c.height = img.height;
        const g = c.getContext("2d")!;
        g.filter = "brightness(0.5) saturate(0.55) contrast(1.05)";
        g.drawImage(img, 0, 0);
        c.toBlob((b) => resolve(b ? URL.createObjectURL(b) : TEXTURE), "image/jpeg", 0.9);
      };
      img.onerror = () => resolve(TEXTURE);
      img.src = TEXTURE;
    });
    return darkTexture;
  }
  const OCEAN_TILES = (x: number, y: number, l: number) =>
    `https://server.arcgisonline.com/ArcGIS/rest/services/Ocean/World_Ocean_Base/MapServer/tile/${l}/${y}/${x}`;

  const visible = $derived(app.filtered.filter((d) => d.lat != null && d.lon != null));
  const visibleTracks = $derived.by(() => {
    const ids = new Set(app.filtered.map((d) => d.id));
    return app.tracks.filter((t) => ids.has(t.deployment));
  });

  /** A colour at half opacity (points without detections). */
  function fade(hex: string) {
    const n = parseInt(hex.slice(1), 16);
    return `rgba(${n >> 16}, ${(n >> 8) & 255}, ${n & 255}, 0.45)`;
  }

  function esc(s: string) {
    return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
  }

  function tooltip(d: Deployment) {
    const where = [d.project, d.site, d.region].filter(Boolean).map((s) => esc(s!)).join(" · ");
    const dets = d.n_detections
      ? `${fmt.count(d.n_detections)} detections`
      : `<span class="muted">No detections</span>`;
    return `<div class="tip"><div class="tip-title">${esc(d.deployment_id)}</div>
      <div>${where}</div>
      <div class="muted">${fmt.date(d.t_deploy)} – ${fmt.date(d.t_recover)}</div>
      <div>${esc(d.instrument_type ?? "Unknown instrument")} · ${esc(d.platform)}</div>
      <div>${dets}</div></div>`;
  }

  /** Rectangle as a GeoJSON polygon, edges densified so they follow the globe. */
  function areaFeature(a: Area) {
    const east = a.west <= a.east ? a.east : a.east + 360;
    const ring: [number, number][] = [];
    const steps = (from: number, to: number) => Math.max(2, Math.ceil(Math.abs(to - from)));
    const n1 = steps(a.west, east);
    for (let i = 0; i <= n1; i++) ring.push([a.west + ((east - a.west) * i) / n1, a.south]);
    const n2 = steps(a.south, a.north);
    for (let i = 1; i <= n2; i++) ring.push([east, a.south + ((a.north - a.south) * i) / n2]);
    for (let i = 1; i <= n1; i++) ring.push([east - ((east - a.west) * i) / n1, a.north]);
    for (let i = 1; i < n2; i++) ring.push([a.west, a.north - ((a.north - a.south) * i) / n2]);
    ring.push(ring[0]);
    const wrap = ring.map(([x, y]) => [x > 180 ? x - 360 : x, y]);
    return { type: "Feature", geometry: { type: "Polygon", coordinates: [wrap] } };
  }

  function box(a: { lat: number; lng: number }, lat: number, lng: number): Area {
    // The shorter way round decides which side of the antimeridian the box is on.
    const [w, e] = a.lng <= lng ? [a.lng, lng] : [lng, a.lng];
    const [south, north] = [Math.min(a.lat, lat), Math.max(a.lat, lat)];
    return e - w <= 180 ? { south, north, west: w, east: e } : { south, north, west: e, east: w };
  }

  function corner(lat: number, lng: number) {
    if (!firstCorner) {
      firstCorner = { lat, lng };
      return;
    }
    app.area = box(firstCorner, lat, lng);
    stopDrawing();
  }

  function stopDrawing() {
    app.drawingArea = false;
    firstCorner = null;
    rubberBand = null;
  }

  /** Where on the globe a pointer event is, or null if it's off the globe. */
  function globeAt(e: PointerEvent) {
    const r = el.getBoundingClientRect();
    return globe?.toGlobeCoords(e.clientX - r.left, e.clientY - r.top) ?? null;
  }

  export function resetView() {
    const pts = visible.length ? visible : app.deployments.filter((d) => d.lat != null);
    if (!globe || !pts.length) return;
    // Mean position on the sphere, so data either side of 180° doesn't average to 0°.
    let x = 0, y = 0, z = 0;
    for (const d of pts) {
      const la = (d.lat! * Math.PI) / 180, lo = (d.lon! * Math.PI) / 180;
      x += Math.cos(la) * Math.cos(lo);
      y += Math.cos(la) * Math.sin(lo);
      z += Math.sin(la);
    }
    const lat = (Math.atan2(z, Math.hypot(x, y)) * 180) / Math.PI;
    const lng = (Math.atan2(y, x) * 180) / Math.PI;
    globe.pointOfView({ lat, lng, altitude: 2.2 }, 1200);
  }

  onMount(() => {
    const g = new Globe(el, { rendererConfig: { antialias: true, alpha: true } })
      .bumpImageUrl("/textures/earth-topology.png")
      .backgroundImageUrl("/textures/night-sky.png")
      .showAtmosphere(true)
      .atmosphereColor("#3fb6d8")
      .atmosphereAltitude(0.17)
      .pointLat("lat")
      .pointLng("lon")
      .pointResolution(16)
      .pointsTransitionDuration(0)
      .pointLabel((d: object) => tooltip(d as Deployment))
      .onPointClick((d: object) => {
        if (!app.drawingArea) app.selectedId = (d as Deployment).id;
      })
      .onPointHover((d: object | null) => (el.style.cursor = d ? "pointer" : ""))
      .pathPoints("points")
      .pathPointLat((p: [number, number]) => p[0])
      .pathPointLng((p: [number, number]) => p[1])
      .pathPointAlt(0.004)
      .pathTransitionDuration(0)
      .pathLabel((t: object) => {
        const d = app.byId.get((t as Track).deployment);
        return d ? tooltip(d) : "";
      })
      .onPathClick((t: object) => {
        if (!app.drawingArea) app.selectedId = (t as Track).deployment;
      })
      .onPathHover((t: object | null) => (el.style.cursor = t ? "pointer" : ""))
      .ringColor(() => (t: number) => `rgba(255,255,255,${1 - t})`)
      .ringMaxRadius(2.5)
      .ringPropagationSpeed(1.6)
      .ringRepeatPeriod(1100)
      .polygonCapColor(() => "rgba(0,0,0,0)")
      .polygonSideColor(() => "rgba(0,0,0,0)")
      .polygonStrokeColor(() => "#7ee0c3")
      .polygonAltitude(0.003);
    g.controls().zoomSpeed = 1.2;
    g.onZoom((pov: { altitude: number }) => {
      // Only re-render the points when the size would visibly change.
      if (Math.abs(pov.altitude - altitude) / altitude > 0.08) altitude = pov.altitude;
    });
    globe = g;

    const ro = new ResizeObserver(() => g.width(el.clientWidth).height(el.clientHeight));
    ro.observe(el);
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && app.drawingArea) stopDrawing();
    };
    window.addEventListener("keydown", key);

    // Area corners: a click (not a drag, which rotates the globe) while drawing.
    let down: { x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => (down = { x: e.clientX, y: e.clientY });
    const onUp = (e: PointerEvent) => {
      if (!app.drawingArea || !down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 5) return;
      const c = globeAt(e);
      if (c) corner(c.lat, c.lng);
    };
    const onMove = (e: PointerEvent) => {
      if (!app.drawingArea || !firstCorner) return;
      const c = globeAt(e);
      if (c) rubberBand = box(firstCorner, c.lat, c.lng);
    };
    el.addEventListener("pointerdown", onDown);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointermove", onMove);
    return () => {
      ro.disconnect();
      window.removeEventListener("keydown", key);
      el.removeEventListener("pointerdown", onDown);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointermove", onMove);
      g._destructor();
    };
  });

  // Points: redrawn when the filter, colours or selection change.
  $effect(() => {
    if (!globe) return;
    const sel = app.selectedId;
    // Points keep a similar size on screen as you zoom.
    const radius = Math.min(1.2, Math.max(0.05, 0.36 * altitude));
    app.colorBy;
    app.legend;
    globe
      .pointColor((o: object) => {
        const d = o as Deployment;
        const c = app.colorOf(d);
        return d.id === sel ? "#ffffff" : d.n_detections ? c : fade(c);
      })
      .pointAltitude((o: object) => ((o as Deployment).id === sel ? 0.05 : 0.01))
      .pointRadius((o: object) => ((o as Deployment).id === sel ? 1.5 : 1) * radius)
      .pointsData(visible);
  });

  $effect(() => {
    if (!globe) return;
    const sel = app.selectedId;
    app.legend;
    globe
      .pathColor((t: object) => {
        const d = app.byId.get((t as Track).deployment);
        return (t as Track).deployment === sel ? "#ffffff" : d ? app.colorOf(d) : "#8a94a6";
      })
      .pathStroke((t: object) => ((t as Track).deployment === sel ? 2.5 : 1.2))
      .pathsData(visibleTracks);
  });

  // Selection: pulse, and fly there if it's off to the side.
  $effect(() => {
    if (!globe) return;
    const d = app.selected;
    if (!d || d.lat == null || d.lon == null) {
      globe.ringsData([]);
      return;
    }
    globe.ringLat(() => d.lat!).ringLng(() => d.lon!).ringsData([{}]);
    const pov = globe.pointOfView();
    globe.pointOfView({ lat: d.lat, lng: d.lon, altitude: Math.min(pov.altitude, 1.4) }, 900);
  });

  $effect(() => {
    if (!globe) return;
    const a = rubberBand ?? app.area;
    globe.polygonsData(a && a.north > a.south ? [areaFeature(a)] : []);
  });

  // Corner marker while drawing.
  $effect(() => {
    if (!globe) return;
    const c = firstCorner;
    globe.labelsData(c ? [c] : []).labelText(() => "").labelDotRadius(0.35).labelColor(() => "#7ee0c3");
  });

  // Leaving draw mode from the sidebar button clears a half-drawn box.
  $effect(() => {
    if (!app.drawingArea) {
      firstCorner = null;
      rubberBand = null;
    }
  });

  $effect(() => {
    if (!globe) return;
    const g = globe as GlobeInstance & { globeTileEngineUrl(f: unknown): GlobeInstance };
    const basemap = app.basemap;
    if (basemap === "ocean") {
      g.globeTileEngineUrl(OCEAN_TILES).globeTileEngineMaxLevel(10);
    } else {
      g.globeTileEngineUrl(null);
      if (basemap === "dark") darkened().then((url) => app.basemap === "dark" && globe!.globeImageUrl(url));
      else globe.globeImageUrl(TEXTURE);
    }
  });

  // First load: look at the data.
  let framed = false;
  $effect(() => {
    if (globe && app.deployments.length && !framed) {
      framed = true;
      resetView();
    }
    if (!app.deployments.length) framed = false;
  });
</script>

<div class="globe" class:drawing={app.drawingArea} bind:this={el}></div>
{#if app.drawingArea}
  <div class="hint">
    {firstCorner ? "Click the opposite corner" : "Click one corner of the area"} · Esc to cancel
  </div>
{/if}
{#if app.basemap === "ocean"}
  <div class="attribution">Basemap: Esri, GEBCO, NOAA, National Geographic, Garmin, HERE and others</div>
{/if}

<style>
  .globe {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .globe.drawing {
    cursor: crosshair !important;
  }
  .hint {
    position: absolute;
    top: 64px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--accent-strong);
    color: #04121b;
    font-weight: 600;
    font-size: 13px;
    padding: 6px 14px;
    border-radius: 999px;
    pointer-events: none;
    box-shadow: var(--shadow);
  }
  .attribution {
    position: absolute;
    right: 8px;
    bottom: 6px;
    font-size: 10px;
    color: var(--text-3);
    pointer-events: none;
  }
  :global(.tip) {
    background: var(--panel-solid);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 10px;
    font: 12px/1.45 var(--font);
    color: var(--text);
    box-shadow: var(--shadow);
    max-width: 280px;
  }
  :global(.tip-title) {
    font-weight: 650;
    margin-bottom: 2px;
  }
  :global(.tip .muted) {
    color: var(--text-3);
  }
</style>
