// Pad Companion download page: points the button straight at the newest release's installer.
// Without network or API access the button still works: it links to the latest release page.

/** "owner/repo", read from <owner>.github.io/<repo>/ when served from GitHub Pages. */
const FALLBACK_REPO = "Eiza-1/stadia-pad-companion";

function repo() {
  const host = location.hostname;
  if (host.endsWith(".github.io")) {
    const owner = host.split(".")[0];
    const name = location.pathname.split("/").filter(Boolean)[0];
    if (name && name !== "download") return `${owner}/${name}`;
  }
  return FALLBACK_REPO;
}

const r = repo();
const $ = (id) => document.getElementById(id);

for (const a of document.querySelectorAll("[data-repo-link]")) a.href = `https://github.com/${r}`;
$("dlBtn").href = $("notesLink").href = `https://github.com/${r}/releases/latest`;

const mb = (bytes) => `${Math.round(bytes / 1048576)} MB`;
const day = (iso) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });

fetch(`https://api.github.com/repos/${r}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
  .then((res) => (res.ok ? res.json() : Promise.reject(res.status)))
  .then((rel) => {
    const exe = rel.assets.find((a) => /setup\.exe$/i.test(a.name)) || rel.assets.find((a) => /\.exe$/i.test(a.name));
    $("notesLink").href = rel.html_url;
    if (!exe) return;
    const btn = $("dlBtn");
    btn.href = exe.browser_download_url;
    btn.setAttribute("download", "");
    const version = rel.tag_name.replace(/^v/, "");
    $("dlLabel").textContent = `Download v${version}`;
    $("dlMeta").textContent = `Free · Windows 10 and 11 · 64-bit · ${mb(exe.size)} · released ${day(rel.published_at)}`;
  })
  .catch(() => { /* keep the release-page link */ });
