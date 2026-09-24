// User preferences, saved on this computer. To add one: give it a default
// here and a control in SettingsDialog.svelte.

const KEY = "nereus.settings";

const DEFAULTS = {
  /** Animated rings around the selected deployment on the globe. */
  pulseSelection: false,
};

export type Settings = typeof DEFAULTS;

function load(): Settings {
  try {
    // Unknown keys from older versions are dropped; new ones get their default.
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    const out = { ...DEFAULTS };
    for (const k of Object.keys(DEFAULTS) as (keyof Settings)[]) {
      if (typeof saved[k] === typeof DEFAULTS[k]) (out as Record<string, unknown>)[k] = saved[k];
    }
    return out;
  } catch {
    return { ...DEFAULTS };
  }
}

export const settings = $state<Settings>(load());

export function resetSettings() {
  Object.assign(settings, DEFAULTS);
}

$effect.root(() => {
  $effect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify(settings));
    } catch {
      // Storage unavailable: settings last for this session only.
    }
  });
});
