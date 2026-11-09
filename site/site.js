// Pad Companion Support: turns the bug / feature forms into pre-filled GitHub issues
// (.github/ISSUE_TEMPLATE/*.yml; field ids below must match the template ids).

/** Fallback when the site isn't served from GitHub Pages: "owner/repo". On <owner>.github.io/<repo>/
 *  the repository is read from the address, so this only matters for other hosts. */
const REPO = "Eiza-1/stadia-pad-companion";

/** GitHub keeps pre-filled links under ~8 KB; longer reports are copied instead. */
const MAX_URL = 7500;

const $ = (id) => document.getElementById(id);
const q = new URLSearchParams(location.search);

function repo() {
  const host = location.hostname;
  if (host.endsWith(".github.io")) {
    const owner = host.split(".")[0];
    const name = location.pathname.split("/").filter(Boolean)[0] || `${owner}.github.io`;
    return `${owner}/${name}`;
  }
  return REPO || null;
}

// ---- routing: #bug, #feature, #limits, #sent; anything else is the start view ----
const VIEWS = ["bug", "feature", "limits", "sent"];
function show() {
  const v = VIEWS.includes(location.hash.slice(1)) ? location.hash.slice(1) : "home";
  for (const s of document.querySelectorAll(".view")) s.hidden = s.id !== `v-${v}`;
  for (const a of document.querySelectorAll(".tabs a")) a.classList.toggle("on", a.dataset.v === v);
  if (v === "bug" || v === "feature") $(`${v}Form`).querySelector("input, textarea")?.focus({ preventScroll: true });
  window.scrollTo({ top: 0 });
}
window.addEventListener("hashchange", show);

// ---- pre-fill from the app (Report a bug opens ?v=0.1.0&conn=Usb&fw=337784&win=...) ----
function prefill() {
  const bug = $("bugForm");
  if (q.get("v")) bug.version.value = q.get("v");
  if (q.get("fw")) bug.firmware.value = q.get("fw");
  if (q.get("win")) bug.windows.value = q.get("win");
  const conn = { Usb: "USB cable", Bluetooth: "Bluetooth", none: "Not connected" }[q.get("conn")];
  if (conn) bug.querySelector(`input[name="connection"][value="${conn}"]`).checked = true;
  if (!bug.windows.value) {
    // Windows 11 reports platformVersion 13+; the user agent alone says "Windows NT 10.0" for both.
    navigator.userAgentData?.getHighEntropyValues?.(["platformVersion"]).then((h) => {
      if (bug.windows.value || navigator.userAgentData.platform !== "Windows") return;
      bug.windows.value = parseInt(h.platformVersion, 10) >= 13 ? "Windows 11" : "Windows 10";
    }).catch(() => {});
  }
}

// ---- submit ----
function required(form) {
  let first = null;
  for (const el of form.querySelectorAll("[required]")) {
    const bad = !el.value.trim();
    el.classList.toggle("invalid", bad);
    el.setAttribute("aria-invalid", String(bad));
    if (bad && !first) first = el;
  }
  first?.focus();
  return !first;
}

function issueUrl(r, template, title, fields) {
  const p = new URLSearchParams({ template, title });
  for (const [k, v] of Object.entries(fields)) if (v) p.set(k, v);
  return `https://github.com/${r}/issues/new?${p}`;
}

function asText(title, fields, labels) {
  return [title, "", ...Object.entries(fields).filter(([, v]) => v).map(([k, v]) => `### ${labels[k]}\n${v}\n`)].join("\n");
}

let copyText = "";
function send(url, short, text) {
  const long = url.length > MAX_URL;
  const target = long ? short : url;
  copyText = long ? text : "";
  $("copyBox").hidden = !long;
  $("reopen").href = target;
  window.open(target, "_blank", "noopener");
  location.hash = "sent";
}

const BUG_LABELS = { "what-happened": "What happened", steps: "Steps to reproduce", frequency: "How often", connection: "Connection",
  "app-version": "Pad Companion version", firmware: "Pad firmware", windows: "Windows version" };
const FEATURE_LABELS = { area: "Area", problem: "What would you like to do?", idea: "Your idea", importance: "How much it matters to you" };

function wire() {
  const r = repo();
  $("setup").hidden = !!r;
  $("allTickets").href = r ? `https://github.com/${r}/issues` : "#";
  for (const b of document.querySelectorAll('button[type="submit"]')) b.disabled = !r;

  for (const el of document.querySelectorAll("[required]")) el.addEventListener("input", () => el.classList.remove("invalid"));

  $("bugForm").addEventListener("submit", (e) => {
    e.preventDefault();
    const f = e.target;
    if (!r || !required(f)) return;
    const title = `[Bug] ${f.summary.value.trim()}`;
    const fields = {
      "what-happened": f.what.value.trim(), steps: f.steps.value.trim(), frequency: f.frequency.value,
      connection: f.connection.value, "app-version": f.version.value.trim(), firmware: f.firmware.value.trim(), windows: f.windows.value.trim(),
    };
    send(issueUrl(r, "bug_report.yml", title, fields), issueUrl(r, "bug_report.yml", title, {}), asText(title, fields, BUG_LABELS));
  });

  $("featureForm").addEventListener("submit", (e) => {
    e.preventDefault();
    const f = e.target;
    if (!r || !required(f)) return;
    const title = `[Feature] ${f.summary.value.trim()}`;
    const fields = { area: f.area.value, problem: f.problem.value.trim(), idea: f.idea.value.trim(), importance: f.importance.value };
    send(issueUrl(r, "feature_request.yml", title, fields), issueUrl(r, "feature_request.yml", title, {}), asText(title, fields, FEATURE_LABELS));
  });

  $("copyBtn").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(copyText);
      $("copyBtn").textContent = "Copied";
    } catch {
      $("copyBtn").textContent = "Couldn't copy";
    }
  });
}

prefill();
wire();
show();
