// Profiles: named sets of mapping, sticks, triggers and rumble. "Default" always exists.

import { settings, profile, save, onSettings, toast } from "../ipc.js";
import { h, panel } from "../ui.js";

let list;

export function init(el) {
  el.innerHTML = `<h1>Profiles</h1><p class="lede">Keep different setups for different games. Switching applies instantly, even mid-game.</p>`;
  const p = panel("Your profiles", "bookmarks");
  list = h(`<div class="plist"></div>`);
  const add = h(`<button class="btn tonal" style="margin-top:14px"><span class="ms">add</span>New profile from current</button>`);
  add.onclick = () => {
    let n = 1;
    while (settings.profiles.some((x) => x.name === `Profile ${n}`)) n++;
    const name = `Profile ${n}`;
    save((s) => {
      s.profiles.push({ ...structuredClone(profile()), name });
      s.active_profile = name;
    });
    toast(`Created ${name}`);
  };
  p.append(list, add);
  p.style.maxWidth = "640px";
  el.append(p);
  onSettings(render);
  render();
}

function render() {
  list.innerHTML = "";
  for (const pr of settings.profiles) {
    const on = pr.name === settings.active_profile;
    const isDefault = pr.name === "Default";
    const item = h(`<div class="pitem ${on ? "on" : ""}"><span class="pname"></span>${on ? `<span class="chiptag">Active</span>` : ""}</div>`);
    item.querySelector(".pname").textContent = pr.name;
    if (!on) {
      const use = h(`<button class="btn text">Use</button>`);
      use.onclick = () => save((s) => (s.active_profile = pr.name));
      item.append(use);
    }
    if (!isDefault) {
      const ren = h(`<button class="ib" aria-label="Rename" title="Rename"><span class="ms">edit</span></button>`);
      ren.onclick = () => rename(item, pr);
      const del = h(`<button class="ib" aria-label="Delete" title="Delete"><span class="ms">delete</span></button>`);
      del.onclick = () => save((s) => {
        s.profiles = s.profiles.filter((x) => x !== pr);
        if (s.active_profile === pr.name) s.active_profile = "Default";
      });
      item.append(ren, del);
    }
    list.append(item);
  }
}

function rename(item, pr) {
  const input = h(`<input maxlength="32" aria-label="Profile name">`);
  input.value = pr.name;
  item.querySelector(".pname").replaceWith(input);
  input.focus();
  input.select();
  const done = (commit) => {
    const name = input.value.trim();
    if (commit && name && name !== pr.name) {
      if (settings.profiles.some((x) => x.name === name)) return toast("That name is taken", "error");
      save((s) => {
        if (s.active_profile === pr.name) s.active_profile = name;
        pr.name = name;
      });
    } else render();
  };
  input.onkeydown = (e) => { if (e.key === "Enter") done(true); if (e.key === "Escape") done(false); };
  input.onblur = () => done(true);
}
