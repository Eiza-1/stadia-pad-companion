// Thin layer over Tauri: commands, events, and one shared settings object with debounced saving.

const tauri = window.__TAURI__;
export const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
export const listen = (event, fn) => tauri.event.listen(event, (e) => fn(e.payload));

/** App version and what this PC offers (Game Bar, Gemini app). Asked once: it scans the Start menu. */
let info = null;
export const appInfo = () => (info ??= invoke("app_info"));

export let settings = null;
export let status = null;
const subs = { settings: [], status: [] };

export const onSettings = (fn) => subs.settings.push(fn);
export const onStatus = (fn) => subs.status.push(fn);

export async function init() {
  [settings, status] = await Promise.all([invoke("get_settings"), invoke("get_status")]);
  listen("status", (s) => {
    status = s;
    subs.status.forEach((f) => f(s));
  });
  // Changed from the tray menu: take it into the shared copy so a later save keeps it.
  listen("autostart", (on) => {
    settings.start_with_windows = on;
    subs.settings.forEach((f) => f(settings));
  });
  return { settings, status };
}

/** The active profile object inside `settings` (mutate it, then call save()). */
export const profile = () =>
  settings.profiles.find((p) => p.name === settings.active_profile) || settings.profiles[0];

let timer;
/** Applies a change to the shared settings, notifies pages, and saves 150 ms after the last change. */
export function save(mutate) {
  if (mutate) mutate(settings);
  subs.settings.forEach((f) => f(settings));
  clearTimeout(timer);
  timer = setTimeout(() => invoke("set_settings", { settings }).catch((e) => toast(`Couldn't save: ${e}`, "error")), 150);
}

let toastTimer;
export function toast(text, icon = "check_circle") {
  const t = document.getElementById("toast");
  t.querySelector(".ms").textContent = icon;
  t.querySelector("span:last-child").textContent = text;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 2600);
}
