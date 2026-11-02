// Shortcuts: keys that do things. Each is
//
//   { keys, when, title, run }
//
// `keys`: what is pressed ("Space", "Shift+Space", "Ctrl+Alt+K", "F2", "ArrowLeft": the modifiers
// Ctrl, Alt, Shift, Meta, then one key); `when`: where it is meant (WHEN: by the kind of document
// the pane last used shows; "any": wherever); `title`: what it does, in a few words; `run`: a
// snippet of JavaScript (TypeScript without its types), run with `app` in scope: the app's
// commands (actions.js: whoever can do a thing by name says so there). It may `await`.
//
// There are the app's own (DEFAULTS) and, over them, the person's: one of theirs with the keys and
// the place of one of the app's is in its place, and with no `run` takes it away. The person's are
// kept in this browser, and are written to and read from a file (the menu: the same list, as
// JSON). An agent changes them for the person through the workspace (the `shortcuts` tool): the
// app tells the workspace what the shortcuts are, what a snippet can call and what was done
// lately (so "a key for what I just did" can be written), and a change an agent offers is put to
// the person here, to take or not: a snippet runs in their page, so none is put there unasked.
//
// A key pressed where something is being typed is typing, and is not looked at.

import { $, el, esc, store, stored } from "./ui.js";
import { app, caused, onDid, recent, says } from "./actions.js";
import { workspace } from "./workspace.js";
import { focused } from "./shell/dock.js";
import { msg } from "./shell/panel.js";

/** Where a shortcut can be meant, and what each is called. */
const WHEN = { any: "anywhere", view: "in a 3D view", sheet: "in a drawing", file: "in a script", board: "on the issue board" };

const DEFAULTS = [
  { keys: "Space", when: "view", title: "Show or hide what is chosen", run: "app.toggle()" },
  { keys: "Shift+Space", when: "view", title: "Show only what is chosen (again: every part)", run: "app.only()" },
];

const MODS = { ctrl: "Ctrl", control: "Ctrl", alt: "Alt", option: "Alt", shift: "Shift", meta: "Meta", cmd: "Meta", command: "Meta", win: "Meta" };
/** Keys as they are kept: the modifiers in one order, then the key ("shift + space" is "Shift+Space"). */
function keysOf(text) {
  const parts = String(text ?? "").split("+").map((p) => p.trim()).filter(Boolean);
  // (The key "+" itself, written last: "Ctrl++".)
  if (/\+\s*\+\s*$/.test(String(text)) || String(text).trim() === "+") parts.push("+");
  const key = parts.pop();
  if (!key || MODS[key.toLowerCase()]) return null;
  const mods = new Set(parts.map((p) => MODS[p.toLowerCase()]));
  if (mods.has(undefined)) return null;
  const named = key.length === 1 ? key.toUpperCase() : /^(space|spacebar)$/i.test(key) ? "Space" : /^esc$/i.test(key) ? "Escape" : key[0].toUpperCase() + key.slice(1);
  return [...["Ctrl", "Alt", "Shift", "Meta"].filter((m) => mods.has(m)), named].join("+");
}
/** The keys a press is: by where the key is on the board for letters, digits and the space bar (so Shift does not change them), else by what it types or is. */
function pressed(e) {
  if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return null;
  const key = e.code === "Space" ? "Space" : /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  return [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Meta", key].filter(Boolean).join("+");
}

// ----- the database ---------------------------------------------------------------------------

/** The person's own, over the app's. */
let mine = stored("topo.shortcuts", []);
const same = (a, b) => a.keys.toLowerCase() === b.keys.toLowerCase() && a.when === b.when;
/** Every shortcut there is: the app's that the person has not changed or taken away, then the person's (`own`). */
export function all() {
  return [...DEFAULTS.filter((d) => !mine.some((m) => same(m, d))), ...mine.filter((m) => m.run).map((m) => ({ ...m, own: true }))];
}
/** A shortcut as it is kept, from what was given; or a string: why it is none. */
function read(s) {
  const keys = keysOf(s?.keys), when = s?.when ?? "any";
  if (!keys) return `keys: what is pressed, as "Space", "Shift+H" or "Ctrl+Alt+ArrowUp" (not ${JSON.stringify(s?.keys ?? null)})`;
  if (!WHEN[when]) return `when: one of ${Object.keys(WHEN).join(", ")} (not ${JSON.stringify(when)})`;
  if (s.run != null && (typeof s.run !== "string" || !s.run.trim())) return "run: a snippet of JavaScript, or null to take the shortcut away";
  return { keys, when, title: String(s.title ?? ""), run: s.run ?? null };
}
/** Sets a shortcut (in place of whatever has its keys in its place); with no `run`, takes that one away. Says why not, if not. */
export function set(given) {
  const s = read(given);
  if (typeof s === "string") return s;
  mine = mine.filter((m) => !same(m, s));
  const def = DEFAULTS.find((d) => same(d, s));
  // (What is the app's own already is not also the person's; one taken away that the app has none of is simply gone.)
  if (def ? !(def.run === s.run && (def.title === s.title || !s.title)) : s.run) mine.push(s);
  changed();
  return null;
}
/** The person's own are let go of: the app's, as they come. */
function reset() { mine = []; changed(); }
function changed() {
  store("topo.shortcuts", mine);
  renderList();
  tell();
}

/** The shortcuts as a file's text: all of them, and those of the app's that were taken away (with no `run`). */
const toFile = () => JSON.stringify({ shortcuts: [...all().map(({ own: _, ...s }) => s), ...mine.filter((m) => !m.run)] }, null, 2) + "\n";
/** Takes up a file's shortcuts, over what there is. Says what was wrong with it, if anything was. */
function fromFile(text) {
  let list;
  try { const j = JSON.parse(text); list = Array.isArray(j) ? j : j.shortcuts; } catch (e) { return `not JSON: ${e.message}`; }
  if (!Array.isArray(list)) return 'no list of shortcuts in it ({ "shortcuts": [{ keys, when, title, run }, …] })';
  const wrong = list.map(set).filter(Boolean);
  return wrong.length ? wrong.join("; ") : null;
}

// ----- keys -------------------------------------------------------------------------------------

const AsyncFunction = (async () => {}).constructor;
/** Runs a shortcut's snippet; what goes wrong in it is said. */
export async function run(s) {
  try { await caused(s.keys, () => new AsyncFunction("app", s.run)(app)); }
  catch (e) { msg(`The shortcut ${s.keys} (${s.title || s.run}) failed: ${e?.message ?? e}`, true); }
}
/** The shortcut some keys are, where the pane last used is what it is: one meant for there, before one meant anywhere. */
function bound(keys) {
  const here = focused(), list = all().filter((s) => s.keys.toLowerCase() === keys.toLowerCase());
  return list.find((s) => s.when === here) ?? list.find((s) => s.when === "any") ?? null;
}
const typing = (t) => t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
let down = null;
document.addEventListener("keydown", (e) => {
  if (e.isComposing || typing(e.target)) return;
  const keys = pressed(e), s = keys && bound(keys);
  if (!s) return;
  // (The key is the shortcut's alone: it does not also scroll the page, or press a button that was clicked last.)
  e.preventDefault();
  down = e.code;
  if (e.target instanceof HTMLButtonElement) e.target.blur();
  if (!e.repeat) run(s);
});
document.addEventListener("keyup", (e) => { if (e.code === down) { down = null; e.preventDefault(); } });

// ----- in the menu: what they are, and their file -------------------------------------------------

function renderList() {
  const box = $("keys");
  if (!box) return;
  box.innerHTML = `<span class="head">Shortcuts</span>` +
    all().map((s, i) => `<div class="key" title="${esc(`${s.run}\n(${WHEN[s.when]}${s.own ? "; your own" : ""})`)}"><kbd>${esc(s.keys)}</kbd><span>${esc(s.title || s.run)} <span class="muted">${s.when === "any" ? "" : esc(WHEN[s.when])}</span></span><span class="x" data-i="${i}" title="Take this shortcut away">×</span></div>`).join("") +
    `<div class="row"><button data-a="load" title="Take up the shortcuts of a file (JSON), over these">Load…</button><button data-a="save" title="Write these to a file (JSON), to keep, edit or load elsewhere">Save…</button><button data-a="reset" title="Let go of your own: the app's shortcuts, as they come">Defaults</button></div>`;
  for (const x of box.querySelectorAll(".x")) x.onclick = () => { const s = all()[Number(x.dataset.i)]; set({ ...s, run: null }); };
  box.querySelector("[data-a=reset]").onclick = () => reset();
  box.querySelector("[data-a=save]").onclick = () => {
    const a = el("a", { href: URL.createObjectURL(new Blob([toFile()], { type: "application/json" })), download: "topo-shortcuts.json" });
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  };
  box.querySelector("[data-a=load]").onclick = () => {
    const pick = el("input", { type: "file", accept: ".json,application/json" });
    pick.onchange = async () => { const f = pick.files[0], wrong = f && fromFile(await f.text()); if (f) msg(wrong ? `Shortcuts from ${f.name}: ${wrong}` : `Shortcuts taken up from ${f.name}.`, !!wrong); };
    pick.click();
  };
}

// ----- with an agent, through the workspace ---------------------------------------------------------

// The workspace is told what the shortcuts are, what a snippet can call and what was done lately: a little after
// any of them changes, and when a workspace that others are in is first there.
let told = null, due = 0;
function tell() {
  clearTimeout(due);
  due = setTimeout(() => {
    if (!workspace.can("shared")) return;
    told = workspace.entry;
    workspace.call("shortcuts", { action: "said", shortcuts: all(), app: says, when: WHEN, recent });
  }, 500);
}
onDid(tell);

// A change an agent offers: put to the person, who takes it or does not.
const answered = new Set();
function renderOffers(offers) {
  const box = $("asks");
  if (!box) return;
  const forMe = offers.filter((o) => o.to === workspace.id && !answered.has(o.n));
  box.innerHTML = forMe.map((o) => {
    const c = o.change, was = all().find((s) => same(s, { keys: keysOf(c.keys) ?? "", when: c.when ?? "any" }));
    const what = c.run == null ? `to take away the shortcut` : was ? `to change the shortcut` : `a shortcut`;
    return `<div class="ask" data-n="${o.n}"><div><b>${esc(o.from)}</b> offers ${what} <kbd>${esc(keysOf(c.keys) ?? c.keys)}</kbd> <span class="muted">${esc(WHEN[c.when ?? "any"] ?? c.when)}</span>${c.title ? `: ${esc(c.title)}` : ""}</div>` +
      (c.run != null ? `<pre>${esc(c.run)}</pre>` : "") + (was ? `<div class="muted">Now: ${esc(was.title || was.run)}</div>` : "") +
      `<div class="row"><button data-a="take" title="It runs in this page when the keys are pressed: take it only if it does what you asked for">Take it</button><button data-a="no">No</button></div></div>`;
  }).join("");
  for (const b of box.querySelectorAll("button")) b.onclick = () => {
    const n = Number(b.closest(".ask").dataset.n), o = forMe.find((x) => x.n === n), take = b.dataset.a === "take";
    const wrong = take ? set(o.change) : null;
    if (wrong) msg(`That shortcut cannot be taken: ${wrong}`, true);
    answered.add(n);
    workspace.call("shortcuts", { action: "answered", n, taken: take && !wrong });
    renderOffers(offers);
  };
}
workspace.onState((s) => {
  if (!s) return;
  if (workspace.can("shared") && told !== workspace.entry) tell();
  renderOffers(workspace.can("shared") ? s.shortcuts ?? [] : []);
});

renderList();
