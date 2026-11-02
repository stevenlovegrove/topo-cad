// The app: the page's frame (shell/: a dock of documents and tools), the views of a project that
// are opened in it (its drawings, its script, its scene in 3D), and what they are views of (the
// project's files and its model as built: project.js; its workspace: workspace.js).

import { $ } from "./ui.js";
import { onWork, versions } from "./work.js";
import { showVersions } from "./scene/proposals.js";
import { idle, step } from "./times.js";
import { RELAYED, model, project, storage } from "./project.js";
import { workspace } from "./workspace.js";
import { initDock, isOpen, openDoc, openProject } from "./shell/dock.js";
import { renderLists } from "./shell/browser.js";
import { showProjects } from "./shell/projects.js";
import { msg, renderPanel, renderSolving, say } from "./shell/panel.js";
import { githubPlace } from "./store.js";
import { auth } from "./auth.js";
import { loadSheets } from "./drawing.js";
import { files, loadSources } from "./editor.js";
import { initScene } from "./scene/index.js";
import { viewKey } from "./scene/views.js";
// (Keys that do things: it listens for them itself.)
import "./shortcuts.js";
import "./machines-tool.js";
import "./nest.js";
import { renderParams } from "./shell/params.js";
import "./buildlog.js";
import { noVersions, showChanges } from "./git-tool.js";
import { offerSessions } from "./share-menu.js";

// The model built (opened, or built again): what shows it is brought up to date (the scene follows the model itself).
model.onChange(async (s, was) => {
  renderPanel();
  renderParams();
  // Opened: its documents as they were left open; the first time, the 3D view (its sheets are drawn after, and do not
  // hold it up).
  if (s.file && s.file !== was?.file) openProject(s.file, () => [viewKey(true)]);
  renderLists();
  // Its files are read and its sheets shown, neither waiting for the other (and the 3D view for neither).
  await Promise.all([
    loadSources().then(() => {
      // A project whose model does not build is looked at in its script.
      if (s.file && s.error && files[0] && !isOpen(`file:${files[0].path}`)) openDoc(`file:${files[0].path}`, { tab: true });
      renderLists();
    }),
    loadSheets(),
  ]);
});
// A file of its folder that the model is not built from, changed, or one more or fewer of them: they are read again.
project.onFiles(() => { if (model.state?.file) loadSources(true); });
// It is being built again (the model as it was is shown meanwhile): that, and how far it has come.
model.onSolving(renderSolving);
// Its sheets drawn: they are listed, and those open are shown.
model.onDrawn(async () => { renderPanel(); renderLists(); await loadSheets(); });

// The scene's documents and tools are said, then the dock is put on the page, the tools where they were left.
initScene();
initDock();
// Which project the page is of, as its address says, and nothing else: `?project=ID` (in the location `in` says: the
// host's, unless it says `browser`, `folder` or `github:owner/name@branch`); `?open=ID`, a file looked at by itself in a tab of its own (not among
// those opened last); neither, none (the projects page). One that cannot be opened is said, on the projects page, and
// no other is opened in its place. (Another is gone to by loading the page with it: nothing of the last is left. The
// host has no project of its own: pages of any of them are open at once.)
step("page", "finding where projects are kept");
// (First: if GitHub has just sent the page back, its address is put right before anything reads it.)
await auth.boot();
await storage.boot();
// (Whether the host keeps workspaces: asked whatever is opened.)
step("page", "asking the host about workspaces");
await workspace.probe();
idle("page");
const asked = new URLSearchParams(location.search), alone = asked.get("open");
let entry = alone ?? asked.get("project"), kind = asked.get("in") ?? "host", sessionTrouble = null;
// (In a session, the page is of what the session is of, which its log says. Nothing of this is loaded otherwise.)
if (/^#room=/.test(location.hash)) {
  const { joinRoom, roomInAddress } = await import("./session/enter.js");
  const room = roomInAddress();
  try {
    if (!room) throw new Error("the link is incomplete");
    ({ entry, kind } = await joinRoom(room));
  } catch (e) {
    sessionTrouble = `That session could not be joined: ${e.message ?? e}`;
    entry = null;
    history.replaceState(history.state, "", project.href(null));
  }
}
/** Why the project asked for cannot be opened from where it is, where it cannot. */
const unreachable = () =>
  !["host", "browser", "folder", RELAYED].includes(kind) && !githubPlace(kind) ? `there is no place "${kind}" projects are kept`
  : storage.of(kind) ? null
  : kind === RELAYED ? "it is a session shared by a link, and this address is not all of the link"
  : kind === "host" ? "this page is not served by a topo-cad host, which it is kept on"
  : !storage.folder ? "it is in a folder on this computer, and none has been opened here"
  : `it is in ${storage.folder.label}, which this browser needs your leave to use again`;
if (!entry) showProjects(sessionTrouble ?? auth.trouble ?? "");
else if (unreachable()) {
  history.replaceState(history.state, "", project.href(null));
  showProjects(`${entry} could not be opened: ${unreachable()}`, { entry, kind });
} else {
  storage.use(kind);
  const r = await project.open(entry, { remember: !alone });
  if (r.error) {
    history.replaceState(history.state, "", project.href(null));
    showProjects(`${entry} could not be opened: ${r.error}`);
  } else {
    if (auth.trouble) msg(auth.trouble, true);
    await storage.findWork(entry, () => workspace.myName());
    if (storage.work) {
      branchSaid();
      // (In a session, the menu is that session's: session/enter.js.)
      if (!storage.store.session) offerSessions(storage.work);
    } else noVersions();
  }
}
/** Of a project that has versions: looks once at where the branch is; says where the page is and what is waiting, and
 *  a click there brings up the Changes panel. ⌘Z takes back the last save, where saves can be (in a text field, it is the field's own). */
function branchSaid() {
  const work = storage.work;
  const show = async () => {
    const s = await work.status();
    say("version", [
      `${s.branch} at ${s.commit?.slice(0, 7) ?? ""}`,
      s.viewing && "HEAD detached",
      s.moved && !s.conflicts.length && `behind ${s.upstream}`,
      s.merging && `merging ${s.merging.branch}`,
      s.conflicts.length && `${s.conflicts.length} unmerged`,
      s.edited.length && `${s.edited.length} changed`,
    ].filter(Boolean).join(" · "));
    const chip = $("status-version");
    chip.title = `${s.repo ?? ""} (${s.branch}): click for what has changed, and the branch`;
    chip.style.cursor = "pointer";
    chip.onclick = () => showChanges();
  };
  if (work.can("undo")) window.addEventListener("keydown", async (e) => {
    const k = e.key.toLowerCase(), redo = (k === "z" && e.shiftKey) || k === "y";
    if (!(e.metaKey || e.ctrlKey) || (k !== "z" && !redo) || /^(INPUT|TEXTAREA|SELECT)$/.test(e.target?.tagName ?? "") || e.target?.isContentEditable) return;
    e.preventDefault();
    if (await (redo ? work.redo() : work.undo())) await project.touched();
    else msg(redo ? "Nothing to redo." : "Nothing to undo.");
  });
  onWork(show);
  show();
  // (Looked at, and nothing moved: what has reached the branch comes in when someone says so.)
  work.fetch().catch(() => {});
  work.watch?.();
}
// Two versions of files, as a proposal's sides are shown (wherever a project is kept).
versions.show = showVersions;
// (Said for what it is: the site not answering, or something of the page's own failing.)
model.watch((e) => { console.error("model:", e); $("status").textContent = e instanceof TypeError && /fetch|network/i.test(e.message) ? "server not reachable" : `page error: ${e?.message ?? e}`; });
