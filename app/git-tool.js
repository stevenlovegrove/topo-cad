// The Changes panel, of a project that has versions, wherever they are kept: what has changed, what is in conflict, and the branch, in
// git's terms. Whatever it does, it does by the git tool (git.js), as an agent would, and it offers only what that kind of versions
// says it can do (work.js).

import { el, esc, openMenu } from "./ui.js";
import { addTool, bring, dropTool, retitle } from "./shell/dock.js";
import { msg } from "./shell/panel.js";
import { storage } from "./project.js";
import { onWork, versions } from "./work.js";
import { git } from "./git.js";

const box = el("div", { class: "gittool" });
let status = null, log = null, logOf = null, logOpen = false, branches = null, typed = "", unticked = new Set();
/** What is being done, said at once: most things ask where they are kept, which takes a moment. */
let doing = null;

// (The tool itself, not by way of the workspace, which opens later than the branch does.)
const call = git;
/** Does something, says how it went, and shows what is now so. */
async function run(args, what, quiet) {
  if (doing) return null;
  doing = what;
  render();
  const r = await call(args);
  doing = null;
  if (r.error) msg(r.error, true);
  else if (!quiet) msg(r.text);
  await refresh();
  return r;
}

/** What is so now. One at a time, however many things say it has changed. */
let refreshing = null, again = false, branchesAsked = false;
async function refresh() {
  if (!storage.work) return;
  if (refreshing) { again = true; return refreshing; }
  refreshing = (async () => {
    do {
      again = false;
      status = (await call({ action: "status" })).data ?? status;
      render();
      retitle();
      // (The log is only asked for when there is something new to see in it.)
      if (logOpen && logOf !== `${status?.head}|${status?.commit}`) loadLog();
      // (So that the branch menus open at once the first time too.)
      if (!branches && !branchesAsked) { branchesAsked = true; loadBranches(); }
    } while (again);
  })().finally(() => { refreshing = null; });
  return refreshing;
}
async function loadLog() {
  const of = `${status?.head}|${status?.commit}`;
  logOf = of;
  const r = await call({ action: "log" });
  if (r.error) { logOf = null; return msg(r.error, true); }
  log = r.data;
  render();
}
/** The branches, as last read: there at once; asked again behind that. */
async function loadBranches() {
  const r = await call({ action: "branches" });
  if (r.error) { if (!branches) msg(r.error, true); return; }
  branches = r.data;
}

const label = () => {
  const n = (status?.edited.length ?? 0) + (status?.conflicts.length ?? 0);
  return `Changes${n ? ` <span class="muted">${n}</span>` : ""}${status?.conflicts.length ? ` <i class="mdot bad"></i>` : ""}`;
};

/** A file in conflict, to resolve by hand: its text with both sides between conflict markers. */
function editByHand(path, marked) {
  const d = el("dialog", { class: "gitedit" }, `<div class="gitedithead"><b>${esc(path)}</b></div><textarea spellcheck="false"></textarea><div class="gitedithead"><button class="act primary" data-a="ok">Mark as resolved</button><button class="act" data-a="no">Cancel</button></div>`);
  const text = d.querySelector("textarea");
  text.value = marked ?? "";
  document.body.append(d);
  d.addEventListener("close", () => d.remove());
  d.querySelector("[data-a=no]").onclick = () => d.close();
  d.querySelector("[data-a=ok]").onclick = async () => {
    if (/^(<{7}|={7}|>{7})/m.test(text.value) && !confirm("It still has conflict markers. Use it as it is?")) return;
    d.close();
    await run({ action: "resolve", path, text: text.value }, "Resolving…", true);
  };
  d.showModal();
}

function render() {
  if (!status) { box.innerHTML = `<div class="muted mpad">Reading the branch…</div>`; return; }
  const s = status, parts = [], inTheWay = s.conflicts.length && s.inTheWay !== false, off = doing ? " disabled" : "", origin = esc(s.upstream), can = (a) => s.can.includes(a);
  const button = (a, label, title, disabled = false) => (can(a) ? `<button class="act" data-a="${a}" title="${esc(title)}"${disabled ? " disabled" : off}>${label}</button>` : "");
  parts.push(`<div class="githead"><button class="act gitbranch" data-a="branches" title="git switch: change or create a branch">⑂ ${esc(s.branch)} ▾</button><span class="muted" title="${esc(s.commit ?? "")}">${esc(s.commit?.slice(0, 7) ?? "")}</span><span class="grow"></span>${button("fetch", "Fetch", `git fetch: see where ${s.upstream} is now. Changes no files`)}${button("pull", "Pull", `git pull: bring ${s.upstream} into this branch, merging your uncommitted changes with it (as --autostash)`, s.viewing || !!s.merging)}${s.ahead ? button("push", `Push ${s.ahead}`, `git push: what is committed here, to ${s.upstream}`) : ""}<button class="act" data-a="more" title="More">⋯</button></div>`);
  if (doing) parts.push(`<div class="gitnote gitdoing">${esc(doing)}</div>`);
  if (s.trouble) parts.push(`<div class="gitnote bad">Could not fetch: ${esc(s.trouble)}</div>`);
  if (s.moved && !inTheWay) parts.push(`<div class="gitnote pending">↓ Behind ${origin}. Pull to update.</div>`);
  if (s.viewing) parts.push(`<div class="gitnote pending">HEAD detached at ${esc(s.commit?.slice(0, 7) ?? "")}. <button class="act" data-a="leave"${off}>Switch back</button></div>`);
  if (s.merging) parts.push(`<div class="gitnote pending">Merging ${esc(s.merging.branch)}: commit to conclude. <button class="act" data-a="abort"${off}>Abort</button></div>`);
  if (s.conflicts.length) {
    parts.push(`<div class="gitnote warn">⚠ Conflict, ${esc(s.doing)}. <button class="act" data-a="abort"${off}>Abort</button></div>`);
    for (const path of s.conflicts) parts.push(`<div class="gitfile clash" data-path="${esc(path)}"><div class="gitpath" title="${esc(path)}">${esc(path)}</div><div class="gitbuttons"><button class="act" data-a="compare">Compare in 3D</button><button class="act" data-a="ours" title="git checkout --ours">Use ours</button><button class="act" data-a="theirs" title="git checkout --theirs">Use theirs</button><button class="act" data-a="hand">Edit…</button></div></div>`);
  }
  if (!s.viewing) {
    if (s.edited.length) {
      const all = !!s.merging;
      parts.push(`<div class="gitlist">${s.edited.map((e) => `<label class="gitfile" data-path="${esc(e.path)}"><input type="checkbox"${unticked.has(e.path) && !all ? "" : " checked"}${all ? " disabled" : ""}><span class="gitpath" title="${esc(e.path)}">${esc(e.path)}${e.gone ? ` <span class="muted">(deleted)</span>` : ""}</span><button class="act" data-a="restore" title="git restore: let this change go"${off}>×</button></label>`).join("")}</div>`);
    } else if (!s.merging && !inTheWay) parts.push(`<div class="muted mpad">No changes.</div>`);
    if (s.edited.length && !can("commit")) parts.push(`<div class="gitnote">${esc(storage.work.whyNot("commit"))}</div>`);
    if ((s.edited.length || s.merging) && !inTheWay && can("commit")) {
      if (s.session) parts.push(`<div class="gitnote">These are everyone's in the session, and a commit is of them all.</div>`);
      const ticked = s.edited.filter((e) => !unticked.has(e.path)).length;
      parts.push(`<textarea class="gitmessage" placeholder="Commit message" rows="3">${esc(typed || s.merging?.message || "")}</textarea><div class="githead"><button class="act primary" data-a="commit" title="${s.pushes ? `git commit and git push: a browser has no local commits, so it goes straight to ${origin}` : "git commit"}"${(!ticked && !s.merging) || doing ? " disabled" : ""}>Commit${s.pushes ? " and push" : ""}</button></div>`);
    }
  }
  parts.push(`<div class="githistoryhead"><button class="act" data-a="history" title="git log">${logOpen ? "▾" : "▸"} Log</button></div>`);
  if (logOpen) parts.push(log ? `<div class="gitlog">${log.commits.map((c) => `<div class="gitcommit${c.sha === log.at ? " here" : ""}" data-sha="${esc(c.sha)}" title="${esc(`${c.author} · ${c.date}\n${c.message}`)}"><span class="muted">${esc(c.sha.slice(0, 7))}</span> <span class="gitsubject">${esc(c.message.split("\n")[0])}</span><span class="gitbuttons">${button("checkout", "Checkout", "git checkout: look at this commit, HEAD detached")}${button("revert", "Revert", "git revert --no-commit: take this commit back, in your changes")}</span></div>`).join("")}</div>` : `<div class="muted mpad">Reading the log…</div>`);
  const message = box.querySelector(".gitmessage"), at = message && document.activeElement === message ? message.selectionStart : null;
  box.innerHTML = parts.join("");
  const now = box.querySelector(".gitmessage");
  if (now) {
    now.oninput = () => { typed = now.value; };
    if (at != null) { now.focus(); now.setSelectionRange(at, at); }
  }
  box.querySelectorAll('.gitfile input[type="checkbox"]').forEach((c) => (c.onchange = () => {
    const path = c.closest(".gitfile").dataset.path;
    c.checked ? unticked.delete(path) : unticked.add(path);
    render();
  }));
}

box.addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-a]");
  if (!b) return;
  e.preventDefault();
  const a = b.dataset.a, path = b.closest("[data-path]")?.dataset.path, sha = b.closest("[data-sha]")?.dataset.sha;
  if (a === "fetch") return run({ action: "fetch" }, "Fetching…", true);
  if (a === "pull") return run({ action: "pull" }, "Pulling…");
  if (a === "push") return run({ action: "push" }, "Pushing…");
  if (a === "leave") return run({ action: "switch", name: status.branch }, "Switching…");
  if (a === "abort") { if (status?.merging && !status.conflicts.length && !confirm("Abort this merge, and the changes it made?")) return; return run({ action: "abort" }, "Aborting…"); }
  if (a === "ours" || a === "theirs") return run({ action: "resolve", path, side: a }, "Resolving…", true);
  if (a === "compare" || a === "hand") {
    const { data } = await call({ action: "conflicts" }), f = data?.files?.[path];
    if (!f) return;
    if (a === "hand") return editByHand(path, f.marked);
    return versions.show({ mine: { [path]: f.ours }, theirs: { [path]: f.theirs }, choose: (side) => run({ action: "resolve", path, side: side === "mine" ? "ours" : "theirs" }, "Resolving…", true) });
  }
  if (a === "restore") { if (confirm(`Restore ${path}, letting your changes to it go?`)) await run({ action: "restore", paths: [path] }, "Restoring…"); return; }
  if (a === "commit") {
    const message = box.querySelector(".gitmessage")?.value ?? "", paths = status.merging ? undefined : status.edited.map((x) => x.path).filter((p) => !unticked.has(p));
    const r = await run({ action: "commit", message, paths }, status.pushes ? "Committing and pushing…" : "Committing…");
    if (r && !r.error) { typed = ""; unticked = new Set(); render(); }
    return;
  }
  if (a === "history") {
    logOpen = !logOpen;
    render();
    if (logOpen && logOf !== `${status?.head}|${status?.commit}`) await loadLog();
    return;
  }
  if (a === "checkout") return run({ action: "checkout", commit: sha }, "Checking out…");
  if (a === "revert") return run({ action: "revert", commit: sha }, "Reverting…");
  const { clientX: x, clientY: y } = e;
  if (a === "branches" || a === "more") {
    // (As last read, at once; read again for next time.)
    const had = branches, ready = loadBranches();
    if (!had) await ready;
    const { branches: names = [], current = status.branch } = branches ?? {};
    const can = (what) => status.can.includes(what);
    if (a === "branches") return openMenu(can("switch") ? [
      ...names.map((name) => ({ label: name === current ? `✓ ${name}` : name, run: () => { if (name !== current) run({ action: "switch", name }, `Switching to ${name}…`, true); } })),
      null,
      { label: "Create branch…", run: () => { const name = prompt("git switch -c:"); if (name?.trim()) run({ action: "branch", name: name.trim() }, `Creating ${name.trim()}…`, true); } },
    ] : [{ label: storage.work.whyNot("switch"), cls: "head" }], x, y);
    const others = names.filter((n) => n !== current);
    return openMenu([
      ...(!can("merge") ? [{ label: storage.work.whyNot("merge"), cls: "head" }] : others.length ? [{ label: `Merge into ${current}:`, cls: "head" }, ...others.map((name) => ({ label: name, run: () => run({ action: "merge", name }, `Merging ${name}…`) }))] : [{ label: "No other branches to merge", cls: "head" }]),
      ...(status.edited.length ? [null, { label: `Restore all ${status.edited.length} changed files`, run: () => { if (confirm("Restore all, letting your changes go?")) run({ action: "restore" }, "Restoring…"); } }] : []),
    ], x, y);
  }
});

// (Before the dock is laid out, which keeps only what it knows of, for a page of a project in a place that may have versions: the page's
// address says which place. Where its project turns out to have none, it is taken away again: `noVersions`.)
const here = new URLSearchParams(location.search);
if ((here.has("project") || here.has("open") || /^#room=/.test(location.hash)) && !["browser", "folder"].includes(here.get("in"))) {
  addTool("git", { title: "Changes", label, el: box, side: "right", beside: "checks" });
  onWork(refresh);
  render();
}
/** The page's project has no versions: no panel. */
export const noVersions = () => dropTool("git");
/** Brings the panel into sight. */
export function showChanges() {
  bring("tool:git");
  // (It may be in sight already: so that something is seen to answer.)
  box.classList.remove("gitflash");
  void box.offsetWidth;
  box.classList.add("gitflash");
}
