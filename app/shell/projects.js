// The projects page: every project in every place they are kept (storage.locations: the topo-cad host the page is
// served by, this browser, a folder on this computer, and GitHub, under one header, its repositories found by asking what the App is on), each place a section of its own and its
// projects by the folder each is in, after those opened last; as tiles, each as it looked when it was last built (previews.js), or as a list.
// One clicked is gone to (the page is loaded again with it: project.go); each is a link to it, so one can be opened in
// a tab of its own. A new project is made in a place from it; a folder is opened from it, or let in again; and where
// the page has no host, a host is gone to. Shown over the page when asked for (the project's name in the header, the
// menu), and where the page is of no project (its address names none, or one that could not be opened: why, said).

import { $, esc, store, stored } from "../ui.js";
import { project, storage } from "../project.js";
import { previewOf } from "../previews.js";
import { sharedOrigin, tokenLink } from "../store.js";
import { auth } from "../auth.js";
import { files, live } from "../editor.js";

/** Whether the project open can be left: its scripts' edits not saved are let go of, if there are any. */
export const leaving = () => (discarded ||= !files.some((f) => f.text !== f.saved) || confirm("Discard unsaved script edits?"));
let discarded = false;
// (Nor is the page left, or loaded again, with them, unasked.)
// (In a session, what is typed has gone to the others: it is not lost by leaving.)
window.addEventListener("beforeunload", (e) => { if (!discarded && !live.typed && files.some((f) => f.text !== f.saved)) e.preventDefault(); });

/** The pictures shown, as URLs: let go of when the page is put away or drawn again. */
let pictures = [];
const letGo = () => { for (const u of pictures) URL.revokeObjectURL(u); pictures = []; pictureOf.clear(); };
/** The same, by the project each is of. */
const pictureOf = new Map();
/** A project asked for that could not be opened from where it is (`{ entry, kind }`): gone to once it can be. */
let waiting = null;

/**
 * Shows the projects page, the search ready to type into. `said`: why it is shown (a project not opened); `asked`:
 * that project, where it can be opened once its place can be used (a folder let in again).
 */
export async function showProjects(said, asked = null) {
  $("menu").hidden = true;
  $("projects").hidden = false;
  say(typeof said === "string" ? said : "", true);
  waiting = asked;
  // (Not put away where the page is of no project: there is nothing under it.)
  $("projectsClose").hidden = !project.ofPage();
  $("projectsFind").value = "";
  $("projectsFind").focus();
  showView();
  await renderProjects();
}
/** Puts it away (not where the page is of no project). */
function hideProjects() {
  if (!project.ofPage()) return;
  $("projects").hidden = true;
  letGo();
}
/** Says something at the top of the page (none: nothing): what went wrong (`bad`), or what was done. */
function say(text, bad) {
  $("projectsSaid").textContent = text;
  $("projectsSaid").hidden = !text;
  $("projectsSaid").classList.toggle("ok", !bad);
}

/** When something was, said short: "today", "3 days ago", or its date. */
function ago(ms) {
  if (!ms) return "";
  const days = Math.floor((Date.now() - ms) / 86400000);
  return days < 1 ? "today" : days < 2 ? "yesterday" : days < 30 ? `${days} days ago` : new Date(ms).toLocaleDateString();
}
/** What is shown of a project with no picture yet: the first letters of its name. */
const initials = (name) => name.split(/[^A-Za-z]+/).filter(Boolean).slice(0, 2).map((w) => w[0].toUpperCase()).join("") || "·";

/** A project's tile (`p`: as `project.listIn` has it; `kind`: the place it is in; `where`: said of the place, in Recent). */
function tile(p, kind, where = "", over = "") {
  const open = p.path === project.ofPage() && kind === storage.kind;
  const meta = [where, p.changed ? `changed ${ago(p.changed)}` : where ? "" : p.path].filter(Boolean).join(" · ");
  return `<a class="ptile${open ? " open" : ""}" href="${esc(project.href(p.path, kind))}" data-path="${esc(p.path)}" data-in="${esc(kind)}" data-find="${esc(`${p.name} ${p.path}`.toLowerCase())}" title="${esc(p.path)}${open ? " (open)" : ""}">
    <span class="pshot"><span class="pnone">${esc(initials(p.name))}</span></span>
    <span class="ptext"><span class="pname">${esc(p.name)}${open ? ` <span class="popen">open</span>` : ""}</span><span class="pmeta">${esc(meta)}</span></span>${over}
  </a>`;
}
/** A tile that does something rather than open a project (`act`: what, as the click handler knows it). */
const action = (act, mark, name, meta, title) => `<button class="ptile pact" data-act="${act}" title="${esc(title)}"><span class="pshot"><span class="pnone">${mark}</span></span><span class="ptext"><span class="pname">${esc(name)}</span><span class="pmeta">${esc(meta)}</span></span></button>`;
const grid = (tiles) => `<div class="pgrid">${tiles}</div>`;

/** What each place's section offers before its projects, as it can be used now. */
function actionsOf(l) {
  const fresh = action(`new:${l.kind}`, "+", "New project…", "a folder in projects/", `A new project in ${l.label}: a folder in projects/, with its model in it`);
  if (l.kind === "host") return l.store ? fresh : action("host", "⇄", "Go to a topo-cad host…", "where topo serve is running", "This page is not served by a topo-cad host: go to one (the app is served by it, with its projects)");
  if (l.kind === "folder") {
    const another = action("folder", "▤", storage.folder ? "Open another folder…" : "Open a folder…", "on this computer", "A folder on this computer, its projects listed here (this browser lets the page read and write it)");
    if (!storage.folder) return another;
    if (!l.store) return action("letin", "↻", `Let ${storage.folder.dir.name} in again`, "the browser asks once the page is loaded again", `${l.label}: this browser needs your leave to use it again`) + another;
    return fresh + another;
  }
  return fresh;
}

/** A small button on a header, or over a tile. */
const small = (act, text, title, cls = "") => `<button class="pact pheadact ${cls}" data-act="${esc(act)}" title="${esc(title)}" aria-label="${esc(title)}">${text}</button>`;
/** What is done to a repository's place. */
const repoButtons = (l) => small(`new:${l.kind}`, "+", `A new project in ${l.label}`) + small(`unfollow:${l.kind}`, "×", `No longer list ${l.label} here (edits kept here stay)`);

/** All of GitHub under one header: a repository with one project, its tile among the others; any other under a sub-header of its own. */
function githubSection(places) {
  const signedIn = auth.signedIn(), hidden = storage.hiddenKinds().length;
  const head = [
    signedIn && `<span class="muted pwho">${esc(auth.login())}</span>`,
    signedIn && small("refresh", "⟳", "Look again for the repositories the App is on"),
    signedIn && auth.app.slug && `<a class="pheadact" target="_blank" rel="noopener noreferrer" href="${esc(`${auth.app.login}/apps/${encodeURIComponent(auth.app.slug)}/installations/new`)}" title="Install the App on more repositories, then come back and look again">Install on more…</a>`,
    !signedIn && auth.can() && small("signin", "Sign in", "Goes to GitHub, and comes back here"),
    small("github", "Open by name…", "A branch of a repository on GitHub, by its name: read from there, and committed to if you may"),
    hidden ? small("unhide", `Show ${hidden} put away`, "List again the repositories put away here") : "",
    signedIn && small("signout", "Sign out", "GitHub is told to take this sign-in back, and it is forgotten here. Edits kept here stay"),
  ].filter(Boolean).join("");
  const github = places.filter((l) => l.kind.startsWith("github:")), ones = [], others = [];
  for (const l of github) {
    const said = listing.get(l.kind), ps = [...(said?.projects ?? [])];
    // (A file of the page's project, looked at by itself, in this place: it is no project of its own.)
    if (storage.kind === l.kind && project.ofPage() && !ps.some((p) => p.path === project.ofPage())) ps.push({ name: project.ofPage(), path: project.ofPage(), changed: 0 });
    if (ps.length === 1 && !said?.error) ones.push(tile(ps[0], l.kind, l.label, small(`unfollow:${l.kind}`, "×", `No longer list ${l.label} here (edits kept here stay)`, "ontile")));
    else others.push(`<div class="pgroup ghrepo" data-place="${esc(l.kind)}"><h4 class="phead">${esc(l.label)}${repoButtons(l)}</h4>${said?.error ? `<p class="muted pnone-here">${esc(said.error)}</p>` : !said ? `<p class="muted pnone-here">Reading…</p>` : !ps.length ? `<p class="muted pnone-here">No projects here yet.</p>` : ""}${grid(ps.map((p) => tile(p, l.kind)).join(""))}</div>`);
  }
  const none = !github.length ? `<p class="muted pnone-here">${signedIn ? "The App is not on any repository of yours yet: install it, then look again." : auth.can() ? "Sign in to reach your repositories." : "Open a branch by its name."}</p>` : "";
  return `<section class="pgroup pplace" data-place="github"><h3 class="phead">GitHub${head}</h3>${none}${ones.length ? grid(ones.join("")) : ""}${others.join("")}</section>`;
}

/** Asks what repositories the App is on. */
async function lookForRepositories() {
  try {
    const more = await storage.refreshReachable();
    if (more) say(`${more} more repositories than can be listed here: open one by name.`, false);
    return true;
  } catch (e) {
    storage.reachableAsked = true;
    say(`GitHub would not say which repositories there are: ${e?.message ?? e}`, true);
    return false;
  }
}

/** How many repositories are read at once. */
const READING_AT_ONCE = 4;
/** What each branch on GitHub was last found to have: `{ projects }` or `{ error }`. */
const listing = new Map();
/** Another drawing of the page has begun: what was being read for this one is drawn by that. */
let drawing = 0;

/** Each project's picture, as it is had (not waited for: the tiles are there first). One had already is there at once. */
function showPictures(within) {
  for (const a of within.querySelectorAll(".ptile[data-path]")) {
    const key = `${a.dataset.in}\n${a.dataset.path}`, had = pictureOf.get(key), show = (url) => { a.querySelector(".pshot").innerHTML = `<img src="${url}" alt="">`; };
    if (had) { show(had); continue; }
    previewOf(a.dataset.path, a.dataset.in).then((url) => {
      if (!url) return;
      pictures.push(url);
      pictureOf.set(key, url);
      if (a.isConnected) show(url);
    });
  }
}
/** The GitHub section as it now stands, in place. (Coalesced: repositories may come in together.) */
let drawn = null;
function drawGithub() {
  drawn ??= requestAnimationFrame(() => {
    drawn = null;
    const at = $("projectsList").querySelector('[data-place="github"]');
    if (!at) return;
    const t = document.createElement("template");
    t.innerHTML = githubSection(storage.locations());
    const now = t.content.firstElementChild;
    at.replaceWith(now);
    find();
    showPictures(now);
  });
}
/** Reads each branch on GitHub, a few at a time, each drawn as it comes. */
async function readGithub(mine) {
  const kinds = storage.githubKinds();
  for (const k of listing.keys()) if (!kinds.includes(k)) listing.delete(k);
  const next = kinds[Symbol.iterator]();
  await Promise.all(Array.from({ length: READING_AT_ONCE }, async () => {
    for (let n = next.next(); !n.done && mine === drawing; n = next.next()) {
      listing.set(n.value, await project.listIn(n.value));
      drawGithub();
    }
  }));
}

/** A project's name where its place has not been read: its folder's, for a model of a project of a folder. */
const nameOfPath = (path) => { const parts = path.replace(/\.ts$/, "").split("/"), last = parts.pop(); return last === "model" && parts.length ? parts.pop() : last; };

/** The page, at once: every place but GitHub, whose repositories are read after it is drawn and come in as they are read. */
async function renderProjects() {
  const mine = ++drawing, places = storage.locations(), list = $("projectsList"), local = places.filter((l) => !l.kind.startsWith("github:"));
  const [lists, recent] = await Promise.all([Promise.all(local.map((l) => (l.store ? project.listIn(l.kind) : null))), project.recent()]);
  if (mine !== drawing) return;
  letGo();
  const of = Object.fromEntries(local.map((l, k) => [l.kind, lists[k]?.projects ?? []]));
  // Recent: of any place (one that cannot be used now, or has not been read yet, by its path).
  const labelOf = (kind) => places.find((l) => l.kind === kind)?.label ?? kind;
  const lately = recent.map((r) => {
    const p = (of[r.in] ?? listing.get(r.in)?.projects)?.find((x) => x.path === r.path) ?? { name: nameOfPath(r.path), path: r.path, changed: 0 };
    return tile(p, r.in, places.length > 1 ? labelOf(r.in) : "");
  });
  const sections = local.map((l, k) => {
    const ps = of[l.kind], groups = new Map();
    for (const p of ps) (groups.get(p.group) ?? groups.set(p.group, []).get(p.group)).push(p);
    // (A file of the page's project, looked at by itself, in this place: it is no project of its own.)
    if (storage.kind === l.kind && project.ofPage() && !ps.some((p) => p.path === project.ofPage())) groups.set("by itself", [{ name: project.ofPage(), path: project.ofPage(), changed: 0 }]);
    const order = [...groups.keys()].sort((a, b) => (b.startsWith("projects")) - (a.startsWith("projects")) || a.localeCompare(b));
    const why = lists[k]?.error ? `<p class="muted pnone-here">${esc(lists[k].error)}</p>` : l.store && !ps.length ? `<p class="muted pnone-here">No projects here yet.</p>` : "";
    const body = order.map((g) => `<h4>${esc(g || "projects")}</h4>${grid(groups.get(g).map((p) => tile(p, l.kind)).join(""))}`).join("");
    return `<section class="pgroup pplace" data-place="${esc(l.kind)}"><h3>${esc(l.label)}</h3>${grid(actionsOf(l))}${why}${body}</section>`;
  });
  list.innerHTML = (lately.length ? `<section class="pgroup recent"><h3>Recent</h3>${grid(lately.join(""))}</section>` : "") + sections.join("") + githubSection(places);
  find();
  showPictures(list);
  // Nothing of GitHub has been waited for. What the App is on, the first time since the page was loaded, and then what each has.
  if (auth.signedIn() && !storage.reachableAsked) {
    readGithub(mine);
    await lookForRepositories();
    if (mine !== drawing) return;
    drawGithub();
  }
  await readGithub(mine);
}

/** Only the projects whose name or path has what is typed in the search (and Recent, and folders with none of them,
 *  not at all; what a place offers, while searching, neither). */
function find() {
  const q = $("projectsFind").value.trim().toLowerCase(), list = $("projectsList");
  for (const a of list.querySelectorAll(".ptile[data-path]")) a.hidden = !!q && !a.dataset.find.includes(q);
  for (const a of list.querySelectorAll(".pact, .pnone-here")) a.hidden = !!q;
  for (const h of list.querySelectorAll("h4")) h.hidden = !!q && !h.nextElementSibling.querySelector(".ptile[data-path]:not([hidden])");
  for (const g of list.querySelectorAll(".pgroup")) g.hidden = !!q && (g.classList.contains("recent") || !g.querySelector(".ptile[data-path]:not([hidden])"));
}

/** Tiles, or a list: as the viewer last chose. */
function showView() {
  const view = stored("topo.projects.view", "tiles");
  $("projectsList").classList.toggle("aslist", view === "list");
  for (const b of $("projectsView").querySelectorAll("button")) b.classList.toggle("on", b.dataset.view === view);
}

/** Goes to a project (by its model's id, in a place), or puts the page away where it is the one the page is of. */
async function go(path, kind, tile) {
  if (path === project.ofPage() && kind === storage.kind) return hideProjects();
  if (!leaving()) return;
  tile?.classList.add("going");
  await project.go(path, kind);
}

/** What a tile that is not a project does, by its `data-act`. */
async function act(what, tile) {
  try {
    if (what.startsWith("new:")) {
      const kind = what.slice(4), name = prompt("Name for the new project (a folder in projects/):");
      if (!name || !leaving()) return;
      const r = await project.create(name, kind);
      if (r.error) return say(`The project was not made: ${r.error}`, true);
      return go(r.file, kind, tile);
    }
    if (what === "folder") {
      await storage.pickFolder();
      say(`${storage.folder.label} is open here: its projects are listed below.`, false);
      return renderProjects();
    }
    if (what === "letin") {
      if (!(await storage.letFolderIn())) return say(`${storage.folder.label} was not let in: this browser was not given leave to use it.`, true);
      // (The project asked for, that it was in: gone to now.)
      if (waiting?.kind === "folder") return go(waiting.entry, "folder", tile);
      say(`${storage.folder.label} is let in again.`, false);
      return renderProjects();
    }
    if (what === "signin") return auth.signIn();
    if (what === "signout") {
      if (!confirm("Sign out of GitHub here? Edits kept in this browser stay.") || !leaving()) return;
      await auth.signOut();
      return location.reload();
    }
    if (what === "refresh") {
      tile?.classList.add("turning");
      // (Asked for, on a page with nothing open from there: each that has no changes kept here is brought up to date.)
      await lookForRepositories();
      await Promise.all(storage.githubKinds().map((k) => storage.of(k).fastForward().catch(() => {})));
      return renderProjects();
    }
    if (what === "unhide") {
      await storage.unhideBranches();
      return renderProjects();
    }
    if (what === "github") {
      $("githubTokenRow").hidden = $("githubTokenSay").hidden = auth.signedIn();
      $("githubTokenKept").textContent = sharedOrigin() ? "This site's address is shared with others, so it is kept only for this tab." : "It is kept in this browser.";
      showTokenLink();
      return $("githubFollow").showModal();
    }
    if (what.startsWith("unfollow:")) {
      await storage.unfollowBranch(what.slice(9));
      return renderProjects();
    }
    if (what === "host") {
      const at = prompt("The address of a topo-cad host (where topo serve is running):", stored("topo.host", "http://127.0.0.1:8765/"));
      if (!at || !leaving()) return;
      const u = new URL(/^https?:\/\//.test(at) ? at : `http://${at}`);
      store("topo.host", u.origin + "/");
      return location.assign(u.origin + "/");
    }
  } catch (e) {
    // (A folder not picked, as asked: nothing is said.)
    if (e?.name !== "AbortError") say(`That did not work: ${e?.message ?? e}`, true);
  }
}

// A project clicked is gone to (with a modifier, or the middle button, the browser opens its link as it would); a tile
// that does something, does it.
$("projectsList").addEventListener("click", (e) => {
  const b = e.target.closest(".pact");
  if (b) { e.preventDefault(); return act(b.dataset.act, b); }
  const a = e.target.closest(".ptile[data-path]");
  if (!a || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  e.preventDefault();
  go(a.dataset.path, a.dataset.in, a);
});
/** The link to make a token, for the repository's owner as typed so far. */
const showTokenLink = () => {
  const owner = $("githubRepo").value.trim().split("/")[0];
  $("githubTokenLink").href = tokenLink(owner);
  $("githubTokenWrite").href = tokenLink(owner, "write");
};
$("githubRepo").addEventListener("input", showTokenLink);
// A branch to follow, as the dialog says it. (The token field is emptied either way: it is kept where it is put, not in the page.)
$("githubFollow").addEventListener("close", async () => {
  const d = $("githubFollow"), [repo, branch, token] = ["githubRepo", "githubBranch", "githubToken"].map((id) => $(id).value.trim());
  $("githubToken").value = "";
  if (d.returnValue !== "follow" || !repo) return;
  try {
    await storage.followBranch(repo, branch || "main", token);
    say("", false);
    await renderProjects();
  } catch (e) {
    say(`That did not work: ${e?.message ?? e}`, true);
  }
});
$("projectsFind").addEventListener("input", find);
// Enter: the first that is found; Escape: the page put away.
$("projectsFind").addEventListener("keydown", (e) => {
  if (e.key === "Enter") { const a = $("projectsList").querySelector(".ptile[data-path]:not([hidden])"); if (a) go(a.dataset.path, a.dataset.in, a); }
});
$("projectsView").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-view]");
  if (b) { store("topo.projects.view", b.dataset.view); showView(); }
});
$("projects").addEventListener("keydown", (e) => { if (e.key === "Escape") hideProjects(); });
$("projectsClose").onclick = hideProjects;
$("title").onclick = () => showProjects();
$("allProjects").onclick = () => showProjects();
