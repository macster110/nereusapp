<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { settings, resetSettings } from "../lib/settings.svelte";

  function close() {
    app.showSettings = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
    <header>
      <h2 id="settings-title">Settings</h2>
      <button class="icon" aria-label="Close" onclick={close}>✕</button>
    </header>

    <div class="content">
      <section>
        <h3>Globe</h3>
        <label class="row">
          <span class="what">
            <span>Pulse the selected deployment</span>
            <small>Animated rings around the deployment you select.</small>
          </span>
          <input type="checkbox" role="switch" class="switch" bind:checked={settings.pulseSelection} />
        </label>
      </section>
    </div>

    <footer>
      <button class="link" onclick={resetSettings}>Restore defaults</button>
      <button class="primary" onclick={close}>Done</button>
    </footer>
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
    width: min(480px, calc(100vw - 32px));
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
    padding: 4px 20px 12px;
  }
  h3 {
    margin: 8px 0 6px;
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13.5px;
  }
  .what small {
    color: var(--text-3);
    font-size: 12px;
  }
  /* A checkbox drawn as a switch. */
  .switch {
    appearance: none;
    flex: none;
    width: 36px;
    height: 20px;
    margin: 0;
    border-radius: 999px;
    background: var(--border-strong);
    position: relative;
    cursor: pointer;
    transition: background 0.15s;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
  }
  .switch:checked {
    background: var(--accent-strong);
  }
  .switch:checked::after {
    transform: translateX(16px);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px 16px;
    border-top: 1px solid var(--border);
  }
</style>
