// What the project has, to open: its drawings and files, as groups of the tree of what it has (objects.js). (Which
// project is open, and where projects are kept: the projects page, projects.js.) A click
// on a row opens it in the pane last used; with shift, in a pane of its own beside that; a row is
// dragged to wherever it should stand.

import { $, openMenu } from "../ui.js";
import { isModelScript, model, project, storage } from "../project.js";
import { viewKey } from "../scene/views.js";
import * as objects from "../objects.js";
import { files } from "../editor.js";
import { machines } from "../machine-panel.js";
import { msg } from "./panel.js";
import { pick } from "../scene/chosen.js";
import { renderIssues } from "../scene/issues.js";
import { beginDrag, isShown, onLayout, openDoc, resetLayout, retitle } from "./dock.js";

// The menu: opened by its button, shut by a click anywhere else or by choosing in it.
$("menuBtn").onclick = (e) => { e.stopPropagation(); $("menu").hidden = !$("menu").hidden; };
$("menu").addEventListener("click", (e) => e.stopPropagation());
$("menu").addEventListener("change", () => { $("menu").hidden = true; });
$("resetLayout").onclick = () => { resetLayout(); $("menu").hidden = true; };
document.addEventListener("click", () => { $("menu").hidden = true; });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") $("menu").hidden = true; });

// The project in the tree of what it has: its name at the root, and its drawings, how it is made and its files as groups.
// (Named as soon as the page is of it, before its model is built: by its folder, or its model's file.)
objects.root(() => ({ name: model.state?.name || (project.ofPage() ? (project.folder() || project.ofPage()).split("/").pop() : "Project"), title: project.ofPage() }));
// A row that opens a document, by its key ("view:" alone: a 3D view, the one there is or another).
const doc = (key, name, opts) => objects.row(1, "doc", key, name, { id: key, noEye: true, on: key !== "view:" && isShown(key), ...opts });
const group = (key, label, rows, count = rows.length) => objects.row(0, "top", key, label, { group: true, noEye: true, note: count || "" }) + (objects.open.has(key) ? rows.join("") : "");
// (First among the drawings: the model itself, to look about in.)
const freeview = () => doc("view:", "Freeview (3D)", { title: "The model in 3D, to look about in (shift-click, or dragged: another view of it)" });
// (Drawings are what the model is: its 3D view and the sheets it says. How it is made is under Build plans.)
objects.section({ key: "drawings", order: -30, rows: () => group("drawings", "Drawings", [freeview(), ...(model.state?.sheets ?? []).map((sh) => doc(`sheet:${sh.number}`, `${sh.number} ${sh.title || ""}`))], model.state?.sheets?.length ?? 0) });

// ----- Build plans (docs/build-plans.md) -----
// The plans script beside the model (`<model>.plans.ts`), evaluated beside it: its tree as written, each layout a
// row that opens it; of alternatives, the option made marked (●), and options the workshop cannot make greyed; and
// every part made once, or said. With no plans script, a suggestion to begin one; and, where the model has sheets
// of stock, nesting them with no plan.
let plansOf = null;          // what model.plans() last said: { file, tree } | { file, none } | { file, error }
let chosen = {};             // of each alternatives, the option made (by their paths), kept beside the model
let workshop = [];           // the machines topo serve knows (none where not served)
const seen = new Set();      // groups shown once: open the first time, as their writer left them after
const plansData = () => project.current()?.replace(/\.ts$/, ".plans.json");
let plansAsked = 0;
async function refreshPlans() {
  // (What was asked before what was asked since is let go of when it comes.)
  const mine = ++plansAsked, stale = () => mine !== plansAsked;
  if (!project.current() || !model.built) { plansOf = null; objects.render(); return; }
  const got = await model.plans();
  let ch = {};
  try { ch = JSON.parse((await storage.store.read(plansData())) ?? "{}").chosen ?? {}; } catch {}
  const ws = (await machines()) ?? [];
  if (stale()) return;
  plansOf = got; chosen = ch; workshop = ws;
  said();
}
// (Not waited for: a build waits for what hears of it, and the plans are asked of the engine that is building.)
model.onChange(() => { refreshPlans(); });
project.onFiles(() => { refreshPlans(); });
/** Whether the workshop can make a node: what it needs (a machine of a kind) is among it. */
const can = (n) => !n.needs?.machine || workshop.some((m) => m.kind === n.needs.machine.kind);
/** Of alternatives, the option made: the one chosen, else the first the workshop can make, else the first. */
const madeOf = (alt) => alt.options.find((o) => o.name === chosen[alt.at]) ?? alt.options.find(can) ?? alt.options[0];
/** How many times each part is made, under the options made. */
function counts(tree) {
  const n = new Map();
  const walk = (x) => {
    if (x.kind === "group") x.children.forEach(walk);
    else if (x.kind === "alternatives") { const o = madeOf(x); if (o) walk(o); }
    else for (const id of x.made ?? []) n.set(id, (n.get(id) ?? 0) + 1);
  };
  tree.children.forEach(walk);
  return n;
}
/** What the build plans say is wrong, among the model's checks (Issues): parts in no plan, parts made twice. */
function said() {
  const out = [];
  const tree = plansOf?.tree;
  if (tree) {
    const n = counts(tree);
    const none = tree.parts.filter((p) => !n.get(p.id)), twice = tree.parts.filter((p) => (n.get(p.id) ?? 0) > 1);
    if (none.length) out.push(`[Plan] ${none.length} part${none.length === 1 ? " is" : "s are"} in no build plan: ${none.map((p) => p.name).join(", ")}`);
    for (const p of twice) out.push(`[Plan] ${p.name} is made ${n.get(p.id)} times, under the options made`);
  } else if (plansOf?.error) out.push(`[Plan] the build plans do not evaluate: ${plansOf.error}`);
  model.planIssues = out;
  renderIssues();
  objects.render();
}
/** Keeps which option of alternatives is made (beside the model), and shows it. */
async function chooseOption(altAt, name) {
  chosen = { ...chosen, [altAt]: name };
  said();
  try { await storage.store.write(plansData(), JSON.stringify({ chosen }, null, 2) + "\n"); } catch (e) { msg(`the option made is not kept: ${e.message ?? e}`, true); }
  objects.render();
}
/** A plans script to begin with: one layout for each material the model's sheets are cut from. */
async function beginPlans() {
  const parts = await model.ask("nest_parts");
  const materials = [...new Set((Array.isArray(parts) ? parts : []).map((p) => p.stock))];
  const stem = project.current().split("/").pop().replace(/\.ts$/, "");
  const text = `// How ${model.state?.name ?? stem} is made (docs/build-plans.md): a tree shown under "Build plans", as it is written
// here. Groups as is logical; layouts (parts nested on sheet stock, on a sheet of a size or a machine's bed);
// alternatives (options, one of which is made: .withMachine({ kind }) says what one needs); elsewhere (bought, made
// by hand). Every part is to be made once, under the options made.
import { plans, layout } from "topo-part/plans";

export default plans(${JSON.stringify(model.state?.name ?? stem)}, [
${materials.map((m) => `  layout(${JSON.stringify(m)}, { parts: { material: ${JSON.stringify(m)} } }),`).join("\n")}
]);
`;
  await storage.store.write(plansOf.file, text);
  await project.touched?.();
  await refreshPlans();
}
objects.on("select", (kind, id) => {
  if (kind === "ghost" && id === "plans:new") beginPlans();
  else if (kind === "info" && listed.has(id)) partsMenu(id, listed.get(id));
});
/** What a row of parts (made elsewhere; in no plan) lists, by its key. */
const listed = new Map();
/** A menu of parts, by the row they are of: one chosen is chosen in the model, and shown in the tree. */
async function partsMenu(key, parts) {
  const r = document.querySelector(`.trow[data-key="${CSS.escape(key)}"]`)?.getBoundingClientRect();
  const mesh = await model.parts();
  const memberOf = (p) => (mesh.members ?? []).find((m) => m.path === p.path || m.path.startsWith(`${p.path}/`))?.path;
  openMenu([
    { label: `${parts.length} part${parts.length === 1 ? "" : "s"}: choose one to see it`, cls: "head" },
    ...parts.map((p) => ({ label: p.name, run: () => { const m = memberOf(p); if (m) { pick("member", m); objects.reveal(m); } else msg(`${p.path}: not found among the model's parts`, true); } })),
  ], r ? r.left + 24 : 200, r ? r.bottom : 200);
}
function planRows() {
  const out = [];
  const scratch = model.state?.stock ? [doc("nest:cut", "Nest flat patterns, with no plan", { title: "The sheets of stock laid flat and nested on a sheet of stock, or on a machine's bed (where served), found as it is watched; a DXF to cut from, or (on a MetalFab) handed to xTool Studio" })] : [];
  if (!plansOf) return scratch;
  if (plansOf.none) {
    if (model.state?.stock) out.push(objects.row(1, "ghost", "plans:new", "+ Plan how it is made: a layout for each material", { id: "plans:new", noEye: true, cls: "ghost", title: `Writes ${plansOf.file}: a plans script beside the model, each of its sheets' materials a layout, to be arranged as is logical` }));
    return [...out, ...scratch];
  }
  if (plansOf.error) return [doc(`file:${plansOf.file}`, `⚠ ${plansOf.error}`, { title: `${plansOf.file} does not evaluate: open it to mend it` }), ...scratch];
  const tree = plansOf.tree, n = counts(tree), names = new Map(tree.parts.map((p) => [p.id, p.name]));
  const node = (x, depth, alt) => {
    const key = `plan:${x.at}`;
    if (x.kind === "group" || x.kind === "alternatives") {
      if (!seen.has(key)) { seen.add(key); objects.open.add(key); }
      const kids = x.kind === "group" ? x.children : x.options;
      out.push(objects.row(depth, "top", key, x.name, { group: true, noEye: true, note: x.kind === "alternatives" ? `${kids.length} options` : "" }));
      if (objects.open.has(key)) for (const k of kids) node(k, depth + 1, x.kind === "alternatives" ? x : null);
      return;
    }
    const twice = (x.made ?? []).filter((id) => (n.get(id) ?? 0) > 1).map((id) => names.get(id));
    const mark = alt ? (madeOf(alt) === x ? "● " : "○ ") : "";
    const needs = x.needs?.machine && !can(x) ? `needs a ${x.needs.machine.kind}` : "";
    const note = [`${x.made?.length ?? 0} parts`, needs, twice.length ? `${twice.join(", ")} in two` : ""].filter(Boolean).join(" · ");
    if (x.kind === "elsewhere") {
      listed.set(key, (x.made ?? []).map((id) => tree.parts.find((p) => p.id === id)).filter(Boolean));
      out.push(objects.row(depth, "info", key, `${mark}${x.name}`, { id: key, noEye: true, note, title: "made elsewhere (bought, by hand): accounted for, not laid out. Click: which parts" }));
    }
    else out.push(objects.row(depth, "doc", `nest:plan/${x.at}`, `${mark}${x.name}`, { id: `nest:plan/${x.at}`, noEye: true, on: isShown(`nest:plan/${x.at}`), note, cls: needs ? "greyed" : "", title: `${x.at}: a layout${x.output === "dxf" ? ", sent out as a DXF" : ""}${needs ? ` (${needs}, and the workshop has none)` : ""}${alt ? " (right-click: make it the option made)" : ""}` }));
  };
  for (const c of tree.children) node(c, 1, null);
  const none = tree.parts.filter((p) => !n.get(p.id));
  if (none.length) {
    listed.set("plan:uncovered", none);
    out.push(objects.row(1, "info", "plan:uncovered", `⚠ ${none.length} part${none.length === 1 ? "" : "s"} in no plan`, { id: "plan:uncovered", noEye: true, cls: "warn", title: `Click: which. ${none.map((p) => p.path).join(", ")}` }));
  }
  return [...out, ...scratch];
}
objects.section({ key: "plans", order: -25, rows: () => group("plans", "Build plans", planRows(), plansOf?.tree ? "" : "") });
/** Of a layout among alternatives (its row's key), the alternatives it is an option of, if it is one. */
function altOf(key) {
  if (!key.startsWith("nest:plan/") || !plansOf?.tree) return null;
  const at = key.slice("nest:plan/".length);
  let found = null;
  const walk = (x) => {
    if (x.kind === "group") x.children.forEach(walk);
    else if (x.kind === "alternatives") { if (x.options.some((o) => o.at === at)) found = { alt: x, name: x.options.find((o) => o.at === at).name }; x.options.forEach(walk); }
  };
  plansOf.tree.children.forEach(walk);
  return found;
}
// (Of a project that is a folder: every script in it, each by where it is in the folder; else the model's and what it imports.)
objects.section({ key: "files", order: -20, rows: () => group("files", "Files", files.map((f) => doc(`file:${f.path}`, project.folder() == null ? f.name.split("/").pop() : f.name, { title: f.built === false ? `${f.path} (the model is not built from it)` : f.path, note: f.text !== f.saved ? "edited" : "" }))) });
/** The right-click menu of a row that opens a document: of a file, to edit it, and (a model or a part) to look at it by itself. */
export function docMenu(key) {
  const items = [{ label: "Open", run: () => openDoc(keyOf(key, false)) }, { label: "Open beside", run: () => openDoc(keyOf(key, true), { beside: true }) }];
  const a = altOf(key);
  if (a && madeOf(a.alt).name !== a.name) items.push(null, { label: `Make "${a.name}" the option made`, run: () => chooseOption(a.alt.at, a.name) });
  const f = key.startsWith("file:") && files.find((x) => x.path === key.slice(5));
  if (f && f.path !== project.current() && isModelScript(f.text)) {
    const part = !/export\s+default/.test(f.text);
    items.push(null, { label: `Open ${part ? "the part" : "the model"} by itself, in a new tab`, run: () => window.open(`${location.pathname}?open=${encodeURIComponent(f.path)}`, "_blank") });
  }
  return items;
}
// A click opens it in the pane last used; with shift, beside that; dragged, wherever it is let go.
const keyOf = (id, fresh) => (id === "view:" ? viewKey(fresh) : id);
objects.on("select", (kind, id) => { if (kind === "doc") openDoc(keyOf(id, false)); });
objects.on("open", (kind, id) => { if (kind === "doc") openDoc(keyOf(id, true), { beside: true }); });
objects.on("drag", (kind, id, e) => { if (kind === "doc") beginDrag(e, keyOf(id, true)); });

/** What the project has is listed again (and which of it is in sight). */
export function renderLists() {
  objects.render();
  retitle();
}
onLayout(renderLists);
