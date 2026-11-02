import { addTool, defineType } from "../shell/dock.js";
import { bindSkeleton } from "./skeleton.js";
// The scene: the open project's model in 3D, with what has been put in it and, where the
// project has them, a scan and other people (workspace.js says what a workspace is, and who
// keeps it). On the page it is:
//   - panes: three 3D views (glview.js) and the issue board; any of the 3D views can be clipped
//     to a slab, which is what a section is;
//   - in the browser: the objects (the model's parts, slabs, polylines; with a scan, it and its
//     selections and planes; issues) and the views kept;
//   - on the right: the issues and their threads, with proposals (alone: the model's checks);
//   - in the header: Back / Forward through views followed, and who is here.
//
// Nobody's view is moved for them. A comment can carry a view link (a partial view state: only
// what matters); following it changes just that and can be undone with Back.
//
// What is only of a scan, or only of a workspace that is shared, is asked of the workspace where
// it is used (`scanned()`, `shared()`): nothing here is set up for one kind of project.

import { $, el } from "../ui.js";
import { reached } from "../times.js";
import { model } from "../project.js";
import * as objects from "../objects.js";
import { workspace } from "../workspace.js";
import { say } from "../shell/panel.js";
import { live, pl, scanned, sel, setLive, setState, views } from "./state.js";
import { freshPlaces, loadMembers, loadRatios, rebuildModel } from "./parts.js";
import { cloudLoaded, forgetPoints, loadCloud, syncHighlights } from "./points.js";
import { syncSlabs } from "./slabs.js";
import { drawing, endDrawing } from "./polylines.js";
import { diff, loadDiff, placeDiffbar, proposalEntry, renderDiffBars } from "./proposals.js";
import { renderLayers, syncLayers } from "./views.js";
import { drawOverlay, initOverlay } from "./overlay.js";
import { rebuildMarks } from "./marks.js";
import { renderChips } from "./chosen.js";
import { bindTree } from "./tree.js";
import { renderChat, renderPeople } from "./people.js";
import { issuesLabel, renderBoard, renderIssues } from "./issues.js";
// (Kinds of thing that nothing here asks for by name: they say what they are, and are asked through kinds.js.)
import "./picked.js";
import "./rooms.js";
import "./commands.js";
import "./drags.js";
import { loadPhotos } from "./photos.js";
import { loadSessions } from "./sessions.js";

let hadScan = null;
/** The workspace's state has been said (again): everything that shows it is brought up to date. */
function onState(s) {
  if (!s) return;
  reached("workspace");
  setState(s);
  // With a scan the model is seen against its points (as glass, on the dark); without one, solid, on paper.
  if (scanned() !== hadScan) {
    hadScan = scanned();
    for (const v of Object.values(views)) v.set({ paper: !hadScan }, true);
  }
  // What was dragged here is in the workspace's state now.
  if (live?.done) setLive(null);
  syncSlabs();
  for (const v of Object.values(views)) { v.zOffset = pl.t[2]; v.floorZ = s.scan?.floor_z ?? 0; }
  const cloudKey = `${s.scan?.path}|${(s.scans ?? []).map((x) => x.address).join()}|${s.scan?.min_range}|${s.scan?.thin}|${s.cloud}|${s.selections.filter((x) => x.ignored).map((x) => x.id).join()}`;
  if (s.cloud && cloudLoaded !== cloudKey) loadCloud(cloudKey);
  // (The project no longer names a scan: its points are not drawn.)
  else if (!s.scan && cloudLoaded) forgetPoints();
  loadMembers();
  // (The proposal shown was applied, rejected or reverted by someone: what it offers is asked for again.)
  if (sel.proposal && diff?.offer && proposalEntry(sel.proposal) && proposalEntry(sel.proposal).state !== diff.offer.state) loadDiff().then(() => { renderDiffBars(); rebuildModel(); });
  freshPlaces();
  say("workspace", s.loading ? `scan: ${s.loading}…` : !workspace.connected ? "the workspace service is not answering" : null);
  if (sel.issue != null && !s.issues.some((i) => i.id === sel.issue)) sel.issue = null;
  syncLayers(); rebuildMarks(); syncHighlights();
  renderChat(); renderPeople(); renderIssues(); renderBoard(); renderLayers(); renderDiffBars(); renderChips();
}

/** Puts the scene on the page, and starts following the project that is open. */
export function initScene() {
  // Tools: the tree of what the project has, and the issues (alone: the model's checks).
  bindTree();
  addTool("project", { title: "Project", el: objects.mount(), side: "left" });
  addTool("issues", { title: "Issues", el: el("div", { id: "issuePanel" }), side: "right", beside: "checks", label: issuesLabel });
  // A document: the issue board. (The 3D views are views.js's.)
  const board = el("div", { id: "boardPane", class: "board" });
  defineType("board", { title: () => "Issue board", make: () => ({ el: board, shown: renderBoard }) });
  // The header: who is here.
  document.querySelector("header").append(el("span", { id: "people" }));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && sel.drafting) { sel.drafting = false; renderIssues(); } });
  // A polyline being drawn: Enter keeps it, Esc drops it.
  document.addEventListener("keydown", (e) => {
    if (!drawing || e.target.closest?.("input, textarea, [contenteditable]")) return;
    if (e.key === "Enter") endDrawing(true);
    if (e.key === "Escape") endDrawing(false);
  });
  // (Panes change size: the toolbars wrap differently.)
  window.addEventListener("resize", () => { for (const v of Object.values(views)) placeDiffbar(v); });
  initOverlay();
  bindSkeleton(() => drawOverlay());
  workspace.onState(onState);
  // (The model built again: its parts, and its checks.)
  model.onChange(() => { loadMembers(); loadRatios(); renderIssues(); loadPhotos(); loadSessions(); });
  // (Its checks found apart, after it is shown: listed when they come.)
  model.onChecked(() => { renderIssues(); objects.render(); });
  // (Parts are coloured by how far they are used, where the checks panel asks for that.)
  for (const id of ["heatMode", "heatCombo"]) $(id).addEventListener("change", loadRatios);
  workspace.watch();
}
