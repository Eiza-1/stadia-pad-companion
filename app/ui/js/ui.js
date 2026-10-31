// Small Material 3 controls shared by the pages. Each returns an element and calls back on change.

export function h(html) {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
}

/** Segmented button. options: [{ value, label, disabled?, exp?, title? }] */
export function seg(options, value, onChange) {
  const el = h(`<div class="seg" role="radiogroup"></div>`);
  const render = (v) => {
    el.innerHTML = "";
    for (const o of options) {
      const b = h(`<button role="radio" class="${o.exp ? "exp" : ""}"></button>`);
      b.textContent = o.label;
      b.setAttribute("aria-checked", String(o.value === v));
      b.classList.toggle("on", o.value === v);
      b.disabled = !!o.disabled;
      b.classList.toggle("dis", !!o.disabled);
      if (o.title) b.title = o.title;
      b.onclick = () => { if (o.disabled || o.value === v) return; render(o.value); onChange(o.value); };
      el.append(b);
    }
  };
  render(value);
  el.set = render;
  return el;
}

/** A settings row with a Material 3 switch. */
export function toggle({ label, desc = "", checked, onChange, disabled = false, note = "" }) {
  const el = h(`<label class="srow"><span class="stext"><b></b><small></small><em></em></span><span class="switch"><input type="checkbox"><i></i></span></label>`);
  el.querySelector("b").textContent = label;
  el.querySelector("small").textContent = desc;
  const input = el.querySelector("input");
  const set = (v, dis = disabled, n = note) => {
    input.checked = !!v;
    input.disabled = dis;
    el.classList.toggle("dis", dis);
    el.querySelector("em").textContent = n;
  };
  set(checked);
  input.onchange = () => onChange(input.checked);
  el.set = set;
  return el;
}

/** Labelled slider with a live value read-out. */
export function slider({ label, min, max, step = 1, value, unit = "%", onInput }) {
  const el = h(`<div class="slider"><div class="slabel"><span></span><output></output></div><input type="range"></div>`);
  el.querySelector("span").textContent = label;
  const input = el.querySelector("input");
  Object.assign(input, { min, max, step, value });
  const out = el.querySelector("output");
  const paint = () => {
    out.textContent = `${input.value}${unit}`;
    input.style.setProperty("--p", `${((input.value - min) / (max - min)) * 100}%`);
  };
  paint();
  input.oninput = () => { paint(); onInput(Number(input.value)); };
  el.set = (v) => { input.value = v; paint(); };
  return el;
}

export function panel(title, icon, body = "") {
  return h(`<section class="panel"><h3>${icon ? `<span class="ms">${icon}</span>` : ""}${title}</h3>${body}</section>`);
}
