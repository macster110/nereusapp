<script lang="ts">
  import { api, message, type ConnectionSettings, type SslMode } from "../lib/api";
  import { app } from "../lib/state.svelte";

  interface Saved extends Omit<ConnectionSettings, "password"> {
    name: string;
  }

  // Connections are remembered on this computer without their passwords.
  const KEY = "nereus.connections";
  function loadSaved(): Saved[] {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? "[]");
    } catch {
      return [];
    }
  }

  const initial = loadSaved();
  let saved = $state<Saved[]>(initial);
  const blank = (): Saved => ({ name: "", host: "localhost", port: 5432, database: "nereus", user: "nereus_reader", ssl: "prefer" });
  let form = $state<Saved>(initial[0] ? { ...initial[0] } : blank());
  let password = $state("");
  let url = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let showUrl = $state(false);

  function pick(s: Saved) {
    form = { ...s };
    password = "";
    error = null;
  }

  function remember() {
    const name = form.name.trim() || `${form.database} @ ${form.host}`;
    const entry = { ...form, name };
    saved = [entry, ...saved.filter((s) => s.name !== name)].slice(0, 12);
    localStorage.setItem(KEY, JSON.stringify(saved));
  }

  function forget(s: Saved) {
    saved = saved.filter((x) => x !== s);
    localStorage.setItem(KEY, JSON.stringify(saved));
  }

  async function fromUrl() {
    error = null;
    try {
      const s = await api.parseConnectionString(url);
      form = { ...form, host: s.host, port: s.port, database: s.database, user: s.user, ssl: s.ssl, name: "" };
      if (s.password) password = s.password;
      showUrl = false;
      url = "";
    } catch (e) {
      error = message(e);
    }
  }

  async function connect(e: Event) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const info = await api.connect({ ...$state.snapshot(form), port: Number(form.port), password: password || null });
      remember();
      app.showConnect = false;
      await app.load(info);
    } catch (err) {
      error = message(err);
    } finally {
      busy = false;
    }
  }

  const SSL: { v: SslMode; label: string }[] = [
    { v: "prefer", label: "Use if available" },
    { v: "require", label: "Required" },
    { v: "verify", label: "Required and verified" },
    { v: "disable", label: "Off" },
  ];
</script>

<div class="backdrop">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="connect-title">
    <div class="intro">
      <img src="/icon.png" alt="" width="56" height="56" />
      <h1 id="connect-title">Connect to a Nereus database</h1>
      <p>Browse deployments on the globe, look at detections over time, and download them for MATLAB, R or Tethys.</p>
      {#if saved.length}
        <h2>Recent</h2>
        <ul class="saved">
          {#each saved as s (s.name)}
            <li class:current={s.name === form.name}>
              <button class="pick" onclick={() => pick(s)}>
                <span>{s.name}</span>
                <small>{s.user}@{s.host}:{s.port}/{s.database}</small>
              </button>
              <button class="forget" aria-label="Forget {s.name}" title="Forget" onclick={() => forget(s)}>✕</button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <form onsubmit={connect}>
      <div class="row">
        <label class="grow"><span>Name <em>(optional)</em></span><input bind:value={form.name} placeholder="e.g. Lab server" /></label>
      </div>
      <div class="row">
        <label class="grow"><span>Host</span><input bind:value={form.host} required autocomplete="off" spellcheck="false" /></label>
        <label class="port"><span>Port</span><input type="number" bind:value={form.port} min="1" max="65535" required /></label>
      </div>
      <div class="row">
        <label class="grow"><span>Database</span><input bind:value={form.database} required spellcheck="false" /></label>
      </div>
      <div class="row">
        <label class="grow"><span>User</span><input bind:value={form.user} required autocomplete="username" spellcheck="false" /></label>
        <label class="grow"><span>Password</span><input type="password" bind:value={password} autocomplete="current-password" /></label>
      </div>
      <div class="row">
        <label class="grow">
          <span>Encryption (SSL)</span>
          <select bind:value={form.ssl}>
            {#each SSL as s (s.v)}<option value={s.v}>{s.label}</option>{/each}
          </select>
        </label>
      </div>

      {#if showUrl}
        <div class="row url">
          <input bind:value={url} placeholder="postgresql://user@host:5432/nereus" spellcheck="false" />
          <button type="button" class="secondary" onclick={fromUrl} disabled={!url.trim()}>Fill in</button>
        </div>
      {:else}
        <button type="button" class="link" onclick={() => (showUrl = true)}>Paste a connection string instead</button>
      {/if}

      {#if error}<p class="error" role="alert">{error}</p>{/if}

      <div class="actions">
        {#if app.info}
          <button type="button" class="secondary" onclick={() => (app.showConnect = false)} disabled={busy}>Cancel</button>
        {/if}
        <button type="submit" class="primary" disabled={busy}>
          {#if busy}<span class="spinner dark"></span> Connecting…{:else}Connect{/if}
        </button>
      </div>
      <p class="note">Passwords are not saved. Read-only access is all the app needs.</p>
    </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: radial-gradient(ellipse at 30% 20%, rgba(22, 103, 168, 0.25), transparent 60%), rgba(3, 6, 12, 0.78);
    display: grid;
    place-items: center;
    z-index: 60;
  }
  .dialog {
    width: min(820px, calc(100vw - 32px));
    display: grid;
    grid-template-columns: 300px 1fr;
    background: var(--panel-solid);
    border: 1px solid var(--border);
    border-radius: 18px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .intro {
    padding: 28px 24px;
    background: linear-gradient(160deg, rgba(22, 103, 168, 0.22), rgba(7, 11, 20, 0) 70%);
    border-right: 1px solid var(--border);
  }
  h1 {
    font-size: 20px;
    font-weight: 650;
    margin: 14px 0 8px;
    line-height: 1.25;
  }
  .intro p {
    color: var(--text-2);
    font-size: 13.5px;
    line-height: 1.5;
    margin: 0;
  }
  h2 {
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
    margin: 22px 0 6px;
  }
  .saved {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 2px;
    max-height: 220px;
    overflow: auto;
  }
  .saved li {
    display: flex;
    border-radius: 8px;
  }
  .saved li:hover,
  .saved li.current {
    background: var(--hover);
  }
  .pick {
    flex: 1;
    min-width: 0;
    text-align: left;
    background: none;
    border: 0;
    color: var(--text);
    padding: 7px 8px;
    display: flex;
    flex-direction: column;
  }
  .pick small {
    color: var(--text-3);
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .forget {
    background: none;
    border: 0;
    color: var(--text-3);
    padding: 0 10px;
    visibility: hidden;
  }
  .saved li:hover .forget {
    visibility: visible;
  }
  form {
    padding: 26px 26px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  label em {
    color: var(--text-3);
    font-style: normal;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .port {
    width: 96px;
  }
  .url input {
    flex: 1;
  }
  .link {
    align-self: flex-start;
  }
  .error {
    color: var(--danger);
    background: rgba(230, 103, 103, 0.08);
    border: 1px solid rgba(230, 103, 103, 0.35);
    border-radius: 8px;
    padding: 8px 10px;
    font-size: 13px;
    margin: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  .note {
    font-size: 11.5px;
    color: var(--text-3);
    margin: 0;
    text-align: right;
  }
  @media (max-width: 760px) {
    .dialog {
      grid-template-columns: 1fr;
    }
    .intro {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
