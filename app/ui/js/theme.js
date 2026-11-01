import { colorPad } from "./pad.js";

// Material You dark schemes seeded from each Stadia pad colour (tonal sets from the approved mockup).
// The pads themselves are product photos (assets/pads); `hi` is the pressed-control highlight on them.

export const SCHEMES = {
  JustBlack: { name: "Just Black", swatch: "#282828",
    glow: "#ff4c1d",
    pri: "#ffb59c", onp: "#5c1900", pc: "#7a2a0e", onpc: "#ffdbcf", sec: "#5d4037", surf: "#141110", l0: "#0e0c0b", l1: "#1c1917", l2: "#211d1b", l3: "#2b2725", l4: "#36312f", on: "#ede0dc", sv: "#d8c2bb", ol: "#53433e" },
  ClearlyWhite: { name: "Clearly White", swatch: "#e7ebea", hi: "#ff4c1d",
    glow: "#d7dcea",
    pri: "#c3c7cf", onp: "#2c3137", pc: "#43474e", onpc: "#dfe2eb", sec: "#41474f", surf: "#121316", l0: "#0d0e10", l1: "#1a1b1e", l2: "#1f2023", l3: "#292a2d", l4: "#343538", on: "#e3e2e6", sv: "#c4c6cf", ol: "#44474e" },
  Wasabi: { name: "Wasabi", swatch: "#cae4db", hi: "#3b6b25",
    glow: "#8fbf6e",
    pri: "#b1d18a", onp: "#1f3701", pc: "#354e16", onpc: "#cdeda3", sec: "#3f4a34", surf: "#11140e", l0: "#0c0f09", l1: "#191d16", l2: "#1e211a", l3: "#282b24", l4: "#33362e", on: "#e1e4d5", sv: "#c4c8ba", ol: "#44483d" },
  NightBlue: { name: "Founder's Edition Night Blue", swatch: "#202634",
    glow: "#4a6fb5",
    pri: "#aac7ff", onp: "#0a305f", pc: "#284777", onpc: "#d6e3ff", sec: "#3d4758", surf: "#111318", l0: "#0c0e12", l1: "#191c20", l2: "#1d2024", l3: "#282a2f", l4: "#33353a", on: "#e2e2e9", sv: "#c4c6d0", ol: "#44474e" },
};

export let current = "JustBlack";

/** Re-seeds the whole app scheme and switches every pad photo (except the Home card, which wipes) to a colour. */
export function applyScheme(color) {
  const p = SCHEMES[color] || SCHEMES.JustBlack;
  current = SCHEMES[color] ? color : "JustBlack";
  const root = document.documentElement.style;
  const map = { "--primary": p.pri, "--on-primary": p.onp, "--pc": p.pc, "--on-pc": p.onpc, "--sec-c": p.sec,
    "--surface": p.surf, "--sc-lowest": p.l0, "--sc-low": p.l1, "--sc": p.l2, "--sc-high": p.l3, "--sc-highest": p.l4,
    "--on-surface": p.on, "--on-sv": p.sv, "--outline": p.ol, "--glow": p.glow };
  for (const [k, v] of Object.entries(map)) root.setProperty(k, v);
  // Pressed-control highlight: a colour that stands out on this pad's photo (default: the accent).
  if (p.hi) root.setProperty("--hi", p.hi);
  else root.removeProperty("--hi");
  document.querySelectorAll("svg.pad:not(.home-pad)").forEach((s) => colorPad(s, current));
  // Rebuilt drawings start neutral: let the live pad state redraw on the one in view.
  window.dispatchEvent(new Event("viewchange"));
}
