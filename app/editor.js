// The script: the open project's files, each to read and edit in a pane of its own, with the
// line a model's error is on marked, and the lines edited since it was saved. A save builds the model again.
//
// A file edited here stays as it is typed, whatever happens to it elsewhere. What is kept may change meanwhile (a pull of
// a branch, another program in a folder): that is said over the text, and a save merges with it, so nothing is overwritten
// that was not seen. What cannot be merged is compared in the 3D views, to say which is to stay.

import { el, esc } from "./ui.js";
import { model, project, storage } from "./project.js";
import { defineType, openOf, retitle } from "./shell/dock.js";
import { renderLists } from "./shell/browser.js";
import { reached } from "./times.js";
import { merger, onWork, versions } from "./work.js";
import { showChanges } from "./git-tool.js";

/** What a session (session/enter.js) has to do with the files, where the page is in one: `text(path)`, a file as it is now typed;
 *  `typed(path, text)`, typing, to share; `changed(path, text)`, set by this file: someone else's typing; `caret(path, line)`;
 *  `lines(path)`: who is on which line, line → [{ name, colour }]; `over(path, from, to, onto)`: what was typed since `from` (`to`)
 *  laid over `onto`, or `onto` where it no longer goes. */
export const live = { text: null, typed: null, changed: null, caret: null, lines: null, over: null };
/** How long a pause in typing is before what was typed goes to the others (ms): what one conflict can take is what went at once. */
const TYPING_PAUSE = 300;

/** The project's files: { path, name, text (as edited), saved (as kept), base (what is kept, as it was when the edits
 *  here began from it) }; and the model's build they were read at. */
export let files = [];
let srcVersion = -1;
const fileOf = (path) => files.find((f) => f.path === path);
/** Whether a file has edits not saved; and whether what is kept has changed since they began. */
const unsaved = (f) => f.text !== f.saved;
const behind = (f) => unsaved(f) && (f.base ?? f.saved) !== f.saved;
/** Merging, once loaded (asked for when a file is first edited): till then, what needs it is not said. */
let sync = null;
const needSync = () => { merger().then((m) => { if (!sync) { sync = m; renderEditors(); } }).catch(() => {}); };
/** The files in conflict, where something being brought in to a branch stopped at them (work.js). */
let held = new Set();
onWork(() => {
  const c = storage.store?.clash, now = new Set(c ? c.paths.filter((p) => !(p in c.chosen)) : []);
  if ([...now].join() !== [...held].join()) { held = now; renderEditors(); }
});
/** What what is kept is called, in what is said of it. */
const elsewhere = () => storage.store?.base?.branch ?? "elsewhere";
/** How long what went well is said, and how long lines that have just arrived are marked (ms). */
const SAID_FOR = 2000, ARRIVED_FOR = 3000;
const KEYWORDS = new Set("import export from const let var function return if else for of in new class extends type interface readonly as default true false null undefined this while switch case break continue throw try catch await async typeof void".split(" "));

function highlight(text) {
  const out = [];
  const re = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:[^"\\\n]|\\.)*"?|'(?:[^'\\\n]|\\.)*'?|`(?:[^`\\]|\\.)*`?)|\b(\d+(?:\.\d+)?)\b|([A-Za-z_$][\w$]*)(?=\s*\()|([A-Za-z_$][\w$]*)/g;
  let last = 0, m;
  while ((m = re.exec(text))) {
    out.push(esc(text.slice(last, m.index)));
    const [t, c, str, num, fn, word] = m;
    if (c) out.push(`<span class="tk-c">${esc(c)}</span>`);
    else if (str) out.push(`<span class="tk-s">${esc(str)}</span>`);
    else if (num) out.push(`<span class="tk-n">${num}</span>`);
    else if (fn) out.push(KEYWORDS.has(fn) ? `<span class="tk-k">${fn}</span>` : `<span class="tk-f">${fn}</span>`);
    else out.push(KEYWORDS.has(word) ? `<span class="tk-k">${word}</span>` : esc(word));
    last = m.index + t.length;
  }
  out.push(esc(text.slice(last)));
  return out.join("") + "\n";
}

/** The line of the model's error in a file, if its message names one (`name.ts:12:5`). */
function errorLine(f) {
  const error = model.state?.error;
  if (!f || !error) return 0;
  const base = f.name.split("/").pop().replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const m = error.match(new RegExp(base + ":(\\d+)")) || (f === files[0] && error.match(/line (\d+)/));
  return m ? Number(m[1]) : 0;
}

/** A file in a pane (by its path): its text to edit, over the same text coloured, beside its line numbers. */
function makeEditor(path) {
  const pane = el("div", { class: "script" }, `<div class="editor"><pre class="gutter"></pre><div class="code"><pre class="srchl"></pre><textarea spellcheck="false" autocapitalize="off" autocomplete="off"></textarea></div><div class="scriptfloat" hidden></div></div>`);
  const src = pane.querySelector("textarea"), hl = pane.querySelector(".srchl"), gutter = pane.querySelector(".gutter"), float = pane.querySelector(".scriptfloat");
  /** What is said over the text for now: `{ text, bad }`. */
  let said = null, saidTimer = null;
  const say = (text, bad = false) => {
    clearTimeout(saidTimer);
    said = text ? { text, bad } : null;
    if (text && !bad) saidTimer = setTimeout(() => say(""), SAID_FOR);
    showFloat();
  };
  let missing = false, arrived = new Set(), arrivedTimer = null;
  const scroll = () => { hl.scrollTop = src.scrollTop; hl.scrollLeft = src.scrollLeft; gutter.scrollTop = src.scrollTop; };
  /** Colours the text as it is, and numbers its lines (the error's marked, and those edited, or just arrived). */
  const paint = () => {
    hl.innerHTML = highlight(src.value);
    const f = fileOf(path), n = src.value.split("\n").length, bad = errorLine(f);
    const edited = new Set(f && unsaved(f) && sync ? sync.changed_lines(f.base ?? f.saved, src.value) : []);
    const others = live.lines?.(path);
    gutter.innerHTML = Array.from({ length: n }, (_, i) => {
      const line = i + 1, cls = line === bad ? "errline" : edited.has(line) ? "edited" : arrived.has(line) ? "arrived" : "";
      // (Who else is on it, so that you can see where an edit of yours may not be kept.)
      const here = (others?.get(line) ?? []).map((p) => `<i class="lineby" style="--who:${esc(p.colour)}" title="${esc(p.name)} is on this line">${esc(p.name.slice(0, 1).toUpperCase())}</i>`).join("");
      return here + (cls ? `<span class="${cls}">${line}</span>` : String(line));
    }).join("\n") + "\n";
    scroll();
  };
  /** The text made this, with the caret where it was in what did not change, and the view where it was. */
  const setText = (text) => {
    const was = src.value, at = src.selectionStart, end = src.selectionEnd, top = src.scrollTop, left = src.scrollLeft;
    let same = 0;
    while (same < was.length && same < text.length && was[same] === text[same]) same++;
    const moved = (k) => (k <= same ? k : Math.max(same, k + text.length - was.length));
    src.value = text;
    src.setSelectionRange(moved(at), moved(end));
    src.scrollTop = top;
    src.scrollLeft = left;
  };
  /** These lines have just changed by something other than typing: marked for a moment. */
  const markArrived = (was, now) => {
    if (!sync) return;
    arrived = new Set(sync.changed_lines(was, now));
    clearTimeout(arrivedTimer);
    arrivedTimer = setTimeout(() => { arrived = new Set(); paint(); }, ARRIVED_FOR);
  };
  /** The same file in other panes, and where it is named as edited or not. */
  const changed = () => {
    for (const other of openOf("file")) if (other.id === path && other.el !== pane) other.render();
    edited();
  };
  /** Your text with what has changed in what is kept since it began; null where the same lines changed. */
  const mergedIn = (f) => sync.merge_file(f.path, f.base ?? f.saved, f.text, f.saved) ?? null;
  /** A file's text made this, by something other than typing. */
  const bringIn = (f, text) => {
    markArrived(f.text, text);
    f.text = text;
    setText(text);
    f.base = f.saved;
    render();
    changed();
  };
  /** What is said and offered over the text, as the file now is. */
  const showFloat = () => {
    const f = fileOf(path), parts = [], where = esc(elsewhere());
    const button = (a, label, title, primary) => `<button class="act${primary ? " primary" : ""}" data-a="${a}" title="${esc(title)}">${esc(label)}</button>`;
    const compare = (a) => button(a, "Compare in 3D", "See both versions in the 3D view, coloured, and say which is to stay");
    if (f && held.has(path)) parts.push(`<div class="floatrow warn"><span>⚠ Conflict: the same lines changed both ways</span>${button("held", "Resolve…", "Go to the Changes panel, to resolve it")}</div>`);
    const clash = f && sync && behind(f) && mergedIn(f) == null;
    if (f && sync && behind(f)) {
      const lines = sync.changed_lines(f.base ?? f.saved, f.saved).length, count = `${lines} line${lines === 1 ? "" : "s"}`;
      parts.push(clash
        ? `<div class="floatrow warn"><span>⚠ ${where} changed the same lines you are editing</span>${compare("versions")}</div>`
        : `<div class="floatrow pending"><span>↓ ${where} changed ${count}: saving merges them</span></div>`);
    }
    if (said) parts.push(`<div class="floatrow${said.bad ? " bad" : " good"}"><span>${esc(said.text)}</span></div>`);
    // (Not while it cannot: which is to stay is asked first.)
    if (f && unsaved(f) && !clash) parts.push(`<div class="floatrow">${button("save", "Save", "Save (⌘S / Ctrl+S): keeps this text, and builds the model again", true)}</div>`);
    const html = parts.join("");
    if (float.dataset.was !== html) { float.innerHTML = html; float.dataset.was = html; }
    float.hidden = !html;
  };
  /** The file's text, as it is now (edited here or in another pane, or read again). */
  const render = () => {
    const f = fileOf(path);
    // (Opened before the project's files are read, as a pane left open is: it has its text when they are.)
    if (!f) { src.value = ""; src.disabled = missing = true; say(files.length ? `${path} is not one of this project's files now` : "reading…", !!files.length); return paint(); }
    if (missing) { missing = false; say(""); }
    src.disabled = false;
    if (src.value !== f.text) {
      // (Read anew, with nothing edited here. Before it has been read at all, nothing has arrived.)
      if (src.value) markArrived(src.value, f.text);
      setText(f.text);
    }
    if (unsaved(f) && !sync) needSync();
    paint();
    showFloat();
  };
  const save = async () => {
    const f = fileOf(path);
    if (!f || !unsaved(f)) return;
    clearTimeout(typing);
    typing = 0;
    f.sent = undefined;
    if (behind(f)) {
      // (Nothing is overwritten that was not seen: what has changed comes in first.)
      if (!sync) { sync = await merger(); }
      const merged = mergedIn(f);
      if (merged == null) return showFloat();
      bringIn(f, merged);
    }
    const text = f.text;
    const r = await project.save(f.path, text);
    if (r.error) return say(r.error, true);
    const now = fileOf(path) ?? f;
    now.saved = now.base = text;
    say(r.model_error ? "Saved: the model has an error" : "Saved", !!r.model_error);
    render();
    changed();
  };
  let typing = 0;
  const sendTyping = () => {
    typing = 0;
    const f = fileOf(path);
    if (!f) return;
    f.sent = undefined;
    live.typed(path, f.text);
  };
  /** Where the caret is, for those in a session. */
  const caretSaid = () => { if (live.caret && document.activeElement === src) live.caret(path, src.value.slice(0, src.selectionStart).split("\n").length); };
  for (const type of ["keyup", "click", "focus"]) src.addEventListener(type, caretSaid);
  src.addEventListener("blur", () => {
    live.caret?.(null);
    // (Moved away: what was typed goes now.)
    if (typing) { clearTimeout(typing); sendTyping(); }
  });
  src.addEventListener("input", () => {
    const f = fileOf(path);
    if (!f) return;
    const was = unsaved(f);
    // (`sent`: what the others have been told of it, till what is typed since goes.)
    if (live.typed) f.sent ??= f.text;
    f.text = src.value;
    if (live.typed) { clearTimeout(typing); typing = setTimeout(sendTyping, TYPING_PAUSE); }
    if (!sync) needSync();
    paint();
    showFloat();
    // (The same file in another pane has the same text.)
    for (const other of openOf("file")) if (other.id === path && other.el !== pane) other.render();
    if (was !== unsaved(f)) edited();
  });
  float.addEventListener("click", async (e) => {
    const a = e.target.closest("button[data-a]")?.dataset.a, f = fileOf(path);
    if (!a || !f) return;
    if (a === "save") return save();
    if (a === "held") return showChanges();
    if (a === "versions") versions.show({ mine: { [path]: f.text }, theirs: { [path]: f.saved }, choose: (side) => {
      if (side === "mine") { f.base = f.saved; render(); }
      else bringIn(f, f.saved);
    } });
  });
  src.addEventListener("scroll", scroll);
  src.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "s") { e.preventDefault(); save(); return; }
    if (e.key === "Tab") {
      e.preventDefault();
      src.setRangeText("  ", src.selectionStart, src.selectionEnd, "end");
      src.dispatchEvent(new Event("input"));
    }
  });
  render();
  return { el: pane, render };
}
defineType("file", {
  title: (path) => path,
  label: (path) => `${esc(path.split("/").pop())}${fileOf(path) && unsaved(fileOf(path)) ? ` <span class="dirty" title="Edited, not saved">●</span>` : ""}`,
  make: makeEditor,
});

/** Which files have edits has changed: said where they are named. */
function edited() {
  retitle();
  renderLists();
}
/** The files as they are now, in every pane that shows one (the model's error may be on another line). */
export function renderEditors() {
  for (const e of openOf("file")) e.render();
}
/** Someone else's typing, in a session: the file has this text now, whoever has it open. */
live.changed = (path, text) => {
  const f = fileOf(path);
  if (!f || text == null) return;
  // (What is typed here and has not gone yet stays, over what has come.)
  if (f.sent != null) { const mine = f.text; f.text = live.over(path, f.sent, mine, text); f.sent = text; }
  else f.text = text;
  // (What is kept is what the model is built from: nothing has "changed elsewhere" that a save would have to merge.)
  f.base = f.saved;
  renderEditors();
};

// The files of the project being opened, as soon as they are listed: read from where they are kept (not waiting for the
// engine, nor for the model to be built), so that they can be looked at at once. Which of them the model is built from
// is said once it is (`loadSources`, which keeps what is edited meanwhile).
project.onListed(async ({ folder, ids }) => {
  if (files.length) return;
  const prefix = folder ? folder + "/" : "";
  files = ids.map((id) => ({ path: id, name: folder != null && id.startsWith(prefix) ? id.slice(prefix.length) : id, text: "", saved: "", reading: true }));
  edited();
  reached("listed");
  const texts = await Promise.all(ids.map((id) => storage.store.read(id).catch(() => null)));
  // (Read again by `loadSources` meanwhile: what it read is newer.)
  if (srcVersion !== -1) return;
  files = files.map((f, k) => (unsaved(f) ? f : { ...f, text: texts[k] ?? "", saved: texts[k] ?? "", base: texts[k] ?? "", reading: false }));
  renderEditors();
  edited();
  reached("files");
});

/**
 * Reads the project's files (again, when the model has been built again, or `again`: its folder has other files
 * than it had, or one the model is not built from has changed): edits made here are kept.
 */
export async function loadSources(again) {
  if (!again && srcVersion === model.state.version && files.length) return renderEditors();
  const r = await project.sources();
  if (!r.files) return;
  const byPath = new Map(files.map((f) => [f.path, f]));
  files = r.files.map((f) => {
    const old = byPath.get(f.path);
    // Keep unsaved edits, and what they began from (in a session, what they are of is told as it comes: nothing is behind).
    if (old && unsaved(old)) return { ...f, saved: f.text, text: old.text, base: live.typed ? f.text : old.base ?? old.saved, sent: old.sent };
    // In a session, as it is now typed, by anyone.
    const typed = live.text?.(f.path);
    if (typed != null) return { ...f, saved: f.text, text: typed, base: f.text };
    // Take what is kept otherwise.
    return { ...f, saved: f.text, base: f.text };
  });
  srcVersion = model.state.version;
  renderEditors();
  edited();
  reached("files");
}
