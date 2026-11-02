import { kind } from "./kinds.js";
import { vis } from "../objects.js";
import { corners, middle } from "./geom.js";
import { openDoc, retitle, setInfo } from "../shell/dock.js";
import { renderTree } from "./tree.js";
import { countOf, keepInSight, keptInSight, rowsOf } from "./homes.js";
// Issues: questions and faults pinned to places, each with a thread, in a shared workspace: the
// list and a thread on the right, the board as a pane. A thread's text is made live (members,
// issues, slabs, polylines, planes and selections it names go there when clicked); a proposal in
// it is shown and acted on from its card. Alone, the list is of the model's own checks, which
// in a shared workspace are a category of issue that lives while a check fails.

import { $, esc } from "../ui.js";
import { vec } from "../glview.js";
import { model } from "../project.js";
import { workspace } from "../workspace.js";
import * as objects from "../objects.js";
import { showTab } from "../shell/panel.js";
import { f3, pinOf, pinnedTo, sel, shared, shortPath, st, toModel, views } from "./state.js";
import { choose, chosen, renderChips, whereOptions } from "./chosen.js";
import { describeView, followLink, gotoIssue, gotoMembers, gotoObject, linkOfView } from "./links.js";
import { rebuildMarks } from "./marks.js";
import { members, rebuildModel } from "./parts.js";
import { badge } from "./people.js";
import { gotoPlane, gotoSelection, planeName, revealSelection, selName, syncHighlights } from "./points.js";
import { lineLength, lineName } from "./polylines.js";
import { actOnProposal, selectProposal } from "./proposals.js";
import { slabName } from "./slabs.js";
import { viewReading } from "./photos.js";

/** Makes what a text refers to (as `refs` marked it) go there when clicked. */
export function bindRefs(box) {
  const on = (q, f) => { for (const a of box.querySelectorAll(q)) a.onclick = (e) => { e.preventDefault(); f(a.dataset); }; };
  on("a[data-member]", (d) => { choose("member", d.member); gotoMembers(d.member); });
  on("a[data-issue]", (d) => openIssue(Number(d.issue)));
  on("a[data-selref]", (d) => gotoSelection(d.selref));
  on("a[data-planeref]", (d) => gotoPlane(d.planeref));
  on("a[data-objref]", (d) => gotoObject(...d.objref.split(":")));
}

const STATUS = { waiting: "Waiting", doing: "In hand", open: "Open", done: "Done" };
// Who an issue waits on: someone, or (an agent asking in its own issue) any person.
const waitingOn = (i) => (i.waiting_on ? workspace.participant(i.waiting_on).name : "a person");
const onMe = (i) => i.status === "waiting" && (i.waiting_on ? i.waiting_on === workspace.id : true);
const PRIORITY = { 1: "high", 2: "normal", 3: "low" };
export const when = (t) => {
  const d = Date.now() / 1000 - t;
  return d < 60 ? "now" : d < 3600 ? `${Math.round(d / 60)} min` : d < 86400 ? `${Math.round(d / 3600)} h` : new Date(t * 1000).toLocaleDateString();
};
/**
 * Of a check that says a reading of a photo does not agree with the rest (`reading-off: photo above: edge of wall
 * #122: off by …`): the photo, and the reading as it is said. None: it is no such check.
 */
const readingOf = (title) => { const m = String(title).match(/reading-off: photo (.+?): (.+): off by /); return m && { photo: m[1], what: m[2] }; };
/** Goes to the reading a check is of: through its photo, the reading lit. Whether it is of one (and there is such a photo). */
const gotoReading = (title) => { const r = readingOf(title); return !!r && viewReading(r.photo, r.what); };
/** The model's own checks (validation, clashes, bearing…) as issues that live only while they fail. */
function checkIssues() {
  const kept = new Set(st.issues.filter((i) => i.category === "check").map((i) => i.title));
  // (The model's own, and what its build plans say: parts in no plan, or in two.)
  return [...((model.built && model.state.issues) || []), ...(model.checksSaid ?? []), ...(model.planIssues ?? [])].filter((t) => !kept.has(String(t))).map((t, k) => ({ id: `c${k}`, title: String(t), category: "check", status: "open", priority: String(t).includes("[Error]") ? 1 : 2, entries: [], ephemeral: true }));
}
function ordered() {
  const col = { doing: 0, open: 1, waiting: 2, done: 3 };
  return [...st.issues].sort((a, b) => col[a.status] - col[b.status] || a.priority - b.priority || a.rank - b.rank || a.id - b.id);
}

export async function openIssue(id) {
  // The issue is what is selected now.
  choose(null);
  sel.issue = id;
  sel.drafting = false;
  showTab("issues");
  // (What was made for it comes into sight with it.)
  renderIssues(); rebuildMarks(); rebuildModel(); renderBoard(); renderTree(); syncHighlights();
  // An issue pinned to a selection shows its points.
  const s = pinnedTo(st.issues.find((i) => i.id === id)?.place, "selection");
  if (s) revealSelection(s);
}

/** What an issue's place is pinned to, in words: the thing, by its kind and its name; or where it is. */
function pinSaid(place) {
  const on = pinOf(place);
  if (!on) return `at model ${f3(toModel(middle(place)))}`;
  if (on.kind === "member") return on.id.startsWith("/") ? `group ${on.id.replace(/^\/|\/$/g, "")}` : `member ${shortPath(on.id)}`;
  if (on.kind === "selection") return `selection ${selName(st.selections?.find((s) => s.id === on.id) ?? { id: on.id })}`;
  if (on.kind === "plane") return `plane ${planeName(st.surfaces?.find((s) => s.id === on.id) ?? { id: on.id })}`;
  return `${on.kind} ${on.id}`;
}

/// Starts a new issue: its title is typed in place; `where` is what it is
/// pinned to ("point", "member", "selection" or "none"; default the likeliest).
export function startDraft(where) {
  sel.issue = null;
  sel.drafting = true;
  sel.where = where ?? whereOptions()[0].key;
  showTab("issues");
  renderIssues();
  $("issuePanel").querySelector("#draftTitle")?.focus();
}

async function createIssue(title, text) {
  const where = whereOptions().some((w) => w.key === sel.where) ? sel.where : "none";
  const args = { action: "create", title, category: where === "none" ? "note" : "scan" };
  if (text) args.text = text;
  // What it is pinned to: a thing, by its kind and its id (a group of parts: as the parts its path picks out); or a point.
  if (where === "point") { args.frame = "scan"; args.point = (sel.pick ?? sel.at).hit; }
  else if (where === "member") args.on = { kind: "member", id: sel.pick?.member?.path ?? sel.member };
  else if (where === "group") args.on = { kind: "member", id: `/${sel.group}/` };
  else if (where !== "none") args.on = { kind: where, id: sel[where] };
  const r = await workspace.call("issues", args);
  if (r.error) return alert(r.error);
  sel.drafting = false;
  choose(null);
  renderChips();
  const id = Number(r.text.match(/#(\d+)/)?.[1]);
  if (id) openIssue(id);
}

/** How many issues there are to see to, for the tool's name: open (with the checks that fail), waiting on this person, and whether a check fails outright. */
let counts = { open: 0, waiting: 0, failing: false };
export const issuesLabel = () => `Issues <span class="count${counts.failing ? " err" : counts.waiting || (!shared() && counts.open) ? " warn" : ""}" title="${counts.waiting ? `${counts.waiting} waiting on you` : ""}">${counts.open || ""}</span>`;

export function renderIssues() {
  const box = $("issuePanel");
  if (!box || !st) return;
  const open = st.issues.filter((i) => i.status !== "done").length + checkIssues().length;
  const waiting = st.issues.filter((i) => onMe(i)).length;
  counts = { open, waiting, failing: checkIssues().some((i) => i.priority === 1) };
  retitle();
  const issue = sel.issue != null && st.issues.find((i) => i.id === sel.issue);
  if (issue) return renderThread(box, issue);
  const all = [...ordered(), ...checkIssues()].filter((i) => (sel.filter === "all" || i.category === sel.filter) && (sel.done || i.status !== "done"));
  const typing = box.querySelector("#draftTitle"), typed = typing?.value ?? "", typingNow = document.activeElement === typing;
  // (Alone, there are the model's own checks to list, and nobody to raise anything with.)
  if (!shared()) {
    box.innerHTML = `<ul class="ilist">${all.map((i) => `<li data-id="${i.id}" class="${i.status}"${readingOf(i.title) ? ` title="Click: look through the photo, at this reading"` : ""}><span class="pri p${i.priority}" title="${PRIORITY[i.priority]} priority"></span><span class="t">${esc(i.title)}</span></li>`).join("") || `<li class="none muted">The model's checks find nothing.</li>`}</ul>`;
    // (A check of a reading of a photo: to the photo, and the reading.)
    for (const li of box.querySelectorAll("li[data-id]")) li.onclick = () => { const chk = all.find((x) => String(x.id) === li.dataset.id); if (chk) gotoReading(chk.title); };
    return;
  }
  box.innerHTML = `
    <div class="ibar">
      <button class="act primary" id="newIssue" title="Raise an issue: on the point, member or selection you are pointing at in a 3D view, or not pinned">New issue…</button>
      <select id="issueFilter" title="Category">${["all", "scan", "note", "check"].map((k) => `<option ${k === sel.filter ? "selected" : ""}>${k}</option>`).join("")}</select>
      <label class="c"><input type="checkbox" id="issueDone" ${sel.done ? "checked" : ""}> done</label>
      <button class="act" id="openBoard" title="The issues as a board, by where each stands (shift-click: beside what is open)">Board</button>
    </div>
    ${sel.drafting ? `<div class="draft"><input type="text" id="draftTitle" placeholder="What is wrong, or what do you want to know?">
      <div class="where"><span class="muted">Pinned to</span>${whereOptions().map((w) => `<button data-where="${w.key}" class="${w.key === sel.where ? "on" : ""}">${esc(w.label)}</button>`).join("")}</div>
      <div class="muted">${whereOptions().length === 1 ? "Click a point, or shift-drag a region, in a 3D view to pin it there. " : ""}Enter to raise it, Esc to cancel.</div></div>` : ""}
    <ul class="ilist">${all.map((i) => `
      <li data-id="${i.id}" class="${i.status}${onMe(i) ? " mine" : ""}"${i.ephemeral && readingOf(i.title) ? ` title="Click: look through the photo, at this reading. Shift-click: keep this check as an issue to discuss"` : ""}>
        <span class="pri p${i.priority}" title="${PRIORITY[i.priority]} priority"></span>
        <span class="t">${i.ephemeral ? "" : `<b>#${i.id}</b> `}${esc(i.title)}</span>
        <span class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="chip ${i.status}">${i.status === "waiting" ? `waiting on ${esc(waitingOn(i))}` : STATUS[i.status]}</span><span class="cat">${i.category}</span></span>
      </li>`).join("") || `<li class="none muted">Nothing here. Pick a point in a 3D view and raise what you see.</li>`}</ul>`;
  $("newIssue").onclick = () => startDraft();
  $("openBoard").onclick = (e) => openDoc("board:issues", { beside: e.shiftKey });
  for (const b of box.querySelectorAll("button[data-where]")) b.onclick = () => { sel.where = b.dataset.where; renderIssues(); $("draftTitle")?.focus(); };
  $("issueFilter").onchange = (e) => { sel.filter = e.target.value; renderIssues(); renderBoard(); };
  $("issueDone").onchange = (e) => { sel.done = e.target.checked; renderIssues(); };
  const dt = $("draftTitle");
  if (dt) { dt.value = typed; if (typingNow) dt.focus(); }
  if (dt) dt.onkeydown = (e) => {
    if (e.key === "Enter" && dt.value.trim()) createIssue(dt.value.trim());
    if (e.key === "Escape") { sel.drafting = false; renderIssues(); e.stopPropagation(); }
  };
  for (const li of box.querySelectorAll("li[data-id]")) li.onclick = async (e) => {
    const id = li.dataset.id;
    if (!id.startsWith("c")) return openIssue(Number(id));
    // A check: keeping it (to discuss it) makes it an issue.
    const chk = checkIssues().find((x) => x.id === id);
    // (One of a reading of a photo: to the photo, and the reading; kept as an issue by a shift-click.)
    if (chk && !e.shiftKey && gotoReading(chk.title)) return;
    if (chk && confirm(`Keep this check as an issue to discuss?\n\n${chk.title}`)) { const r = await workspace.call("issues", { action: "create", title: chk.title, category: "check" }); const n = Number(r.text.match(/#(\d+)/)?.[1]); if (n) openIssue(n); }
  };
}

/// Text with what it refers to made live: a selection by its id (s3) shows that selection; an issue (#4) opens it.
export function refs(text, issue) {
  // Members first: the text between them is then looked at for the rest.
  const found = memberRefs(text, issue);
  if (!found.length) return objectRefs(text);
  let out = "", at = 0;
  for (const m of found) {
    out += objectRefs(text.slice(at, m.start)) + `<a href="#" class="ref" data-member="${esc(m.path)}" title="${esc(m.path)}: select it and go to it">${esc(text.slice(m.start, m.end))}</a>`;
    at = m.end;
  }
  return out + objectRefs(text.slice(at));
}

/**
 * Where a text names members: [{ start, end, path }], in order, not overlapping.
 * A member is named by its group and name ("West wall/stud#3", "T7/web.T1-Cl"),
 * or by its name alone where that says which ("stud#6", "top_chord.X1-P"): the
 * only one of that name, else the one in the group named nearest before it in
 * the text (or after), else in the group of the member the issue is on,
 * else the one nearest the issue's place.
 */
function memberRefs(text, issue) {
  if (!members?.length) return [];
  const short = new Map(), byName = new Map(), groups = new Set();
  for (const m of members) {
    const p = m.path.split("/"), name = p[p.length - 1], group = p[p.length - 2] ?? "";
    short.set(`${group}/${name}`, m.path);
    groups.add(group);
    if (!byName.has(name)) byName.set(name, []);
    byName.get(name).push({ group, path: m.path });
  }
  const out = [], taken = (s, e) => out.some((r) => s < r.end && e > r.start);
  const edge = (i) => i < 0 || i >= text.length || !/[\w#]/.test(text[i]);
  const each = (needle, f) => { for (let i = text.indexOf(needle); i >= 0; i = text.indexOf(needle, i + 1)) if (edge(i - 1) && edge(i + needle.length) && !taken(i, i + needle.length)) f(i); };
  // Group and name together: the longest first, so "stud#12" is not taken as "stud#1".
  for (const s of [...short.keys()].sort((a, b) => b.length - a.length)) if (text.includes(s)) each(s, (i) => out.push({ start: i, end: i + s.length, path: short.get(s) }));
  // Where groups are mentioned, for names alone.
  const mentions = [];
  for (const g of groups) if (g && text.includes(g)) for (let i = text.indexOf(g); i >= 0; i = text.indexOf(g, i + 1)) mentions.push({ at: i, group: g });
  const own = (pinnedTo(issue?.place, "member") ?? "").split("/").slice(-2)[0];
  for (const [name, list] of [...byName].sort((a, b) => b[0].length - a[0].length)) {
    // (Only names that could not be ordinary words.)
    if (!/[#.]/.test(name) || !text.includes(name)) continue;
    each(name, (i) => {
      let pick = list.length === 1 ? list[0] : null;
      if (!pick) {
        const has = mentions.filter((m) => list.some((l) => l.group === m.group));
        const near = has.filter((m) => m.at < i).sort((a, b) => b.at - a.at)[0] ?? has.sort((a, b) => a.at - b.at)[0];
        pick = list.find((l) => l.group === (near?.group ?? own));
        // Else the one of that name nearest the issue's place.
        if (!pick && issue?.place) {
          const c = toModel(middle(issue.place));
          const far = (l) => { const at = members.find((x) => x.path === l.path)?.about?.at; return at ? Math.hypot(...vec.sub(at, c)) : Infinity; };
          pick = list.map((l) => [far(l), l]).sort((a, b) => a[0] - b[0])[0]?.[1];
        }
      }
      if (pick) out.push({ start: i, end: i + name.length, path: pick.path });
    });
  }
  return out.sort((a, b) => a.start - b.start);
}

function objectRefs(text) {
  return esc(text).replace(/(^|[^\w#])(s\d+|p\d+|b\d+|l\d+|#\d+)\b/g, (all, pre, id) => {
    if (id[0] === "b") { const s = st.slabs?.find((x) => x.id === id); return s ? `${pre}<a href="#" class="ref sel" data-objref="slab:${id}" title="Look through this slab">${esc(slabName(s))}</a>` : all; }
    if (id[0] === "l") { const l = st.polylines?.find((x) => x.id === id); return l ? `${pre}<a href="#" class="ref sel" data-objref="polyline:${id}" title="Show this polyline (${lineLength(l).toFixed(2)} m)">${esc(lineName(l))}</a>` : all; }
    if (id[0] === "p") { const pn = st.surfaces?.find((x) => x.id === id); return pn ? `${pre}<a href="#" class="ref sel" data-planeref="${id}" title="Show this plane">${esc(planeName(pn))}</a>` : all; }
    if (id[0] === "#") return st.issues.some((i) => i.id === Number(id.slice(1))) ? `${pre}<a href="#" class="ref" data-issue="${id.slice(1)}">${id}</a>` : all;
    const s = st.selections?.find((x) => x.id === id);
    return s ? `${pre}<a href="#" class="ref sel" data-selref="${id}" title="Show this selection (${s.count.toLocaleString()} points)">${esc(selName(s))}</a>` : all;
  });
}

function entryHtml(issue, e, k) {
  const who = `${badge(e.who, true)} <b>${esc(workspace.participant(e.who).name)}</b> <span class="muted">${when(e.time)}</span>`;
  if (e.kind === "comment") return `<div class="entry comment">${who}<div class="text">${refs(e.text, issue)}</div>${e.view ? `<button class="viewlink" data-view="${k}" title="Show this in a pane (Back returns to your view)">Show: ${esc(e.view_label || "view")}${(() => { const q = e.view.proposal && issue.entries.find((x) => x.kind === "proposal" && x.n === e.view.proposal.n && (e.view.proposal.issue ?? issue.id) === issue.id); return q ? ` · proposal ${q.n} (${esc(q.state)})` : ""; })()}</button>` : ""}</div>`;
  if (e.kind === "tool") return `<div class="entry tool">${badge(e.who, true)} ran <code>${esc(e.name)}</code> <span class="muted">${esc(e.summary)}</span></div>`;
  if (e.kind === "change") return `<div class="entry tool">${badge(e.who, true)} <span class="muted">${esc(e.what)} · ${when(e.time)}</span></div>`;
  if (e.kind === "evidence") return `<div class="entry card">${who}<div class="k">Evidence</div><b>${esc(e.title)}</b><div class="text">${refs(e.text, issue)}</div></div>`;
  if (e.kind === "proposal") {
    const on = sel.proposal?.commit === e.commit;
    const d = (a, b) => (a === b ? `${a}` : `${a} → <b>${b}</b>`);
    return `<div class="entry card proposal ${e.state}${on ? " on" : ""}" id="prop-${issue.id}-${e.n}">${who}
      <div class="k">Proposal ${e.n} · ${e.state}</div><b>${esc(e.title)}</b>
      <div class="muted">${e.files.map(esc).join(", ")} <span class="add">+${e.added}</span> <span class="del">−${e.removed}</span>${e.members_added.length || e.members_removed.length ? ` · members +${e.members_added.length} −${e.members_removed.length}` : ""}</div>
      <div class="tally">supported ${d(e.before.supported, e.after.supported)} · contradicted ${d(e.before.contradicted, e.after.contradicted)} · unseen ${d(e.before.unseen, e.after.unseen)}</div>
      <div class="row"><button data-p="${e.n}" data-a="show">${on ? "Hide" : "Show in views"}</button><button data-p="${e.n}" data-a="diff">Text diff</button>${e.state === "open" ? `<button data-p="${e.n}" data-a="apply" class="go">${sel.sure?.commit === e.commit && Date.now() < sel.sure.until ? "Apply to the files?" : "Apply"}</button><button data-p="${e.n}" data-a="reject">Reject</button>` : e.state === "applied" ? `<button data-p="${e.n}" data-a="revert">${sel.sure?.commit === e.commit && Date.now() < sel.sure.until ? "Revert in the files?" : "Revert"}</button>` : ""}</div>
      <pre class="difftext" id="diff-${issue.id}-${e.n}" hidden></pre></div>`;
  }
  return "";
}

function diffHtml(text) {
  return text.split("\n").filter((l) => !l.startsWith("diff --git") && !l.startsWith("index ")).map((l) => `<span class="${l.startsWith("+") && !l.startsWith("+++") ? "add" : l.startsWith("-") && !l.startsWith("---") ? "del" : l.startsWith("@@") ? "hunk" : ""}">${esc(l)}</span>`).join("\n");
}

function renderThread(box, i) {
  // The page is redrawn when anyone does anything: what is being typed stays as it is.
  const focused = document.activeElement;
  if (focused?.id === "iTitle") return;
  const caret = focused?.id === "reply" ? [focused.selectionStart, focused.selectionEnd] : null;
  const draft = box.querySelector("#reply")?.value ?? "";
  const attach = box.querySelector("#attach")?.checked ?? false;
  const openDiffs = [...box.querySelectorAll("pre.difftext:not([hidden])")].map((p) => [p.id, p.innerHTML]);
  const c = chosen(), referTo = c?.of.refer?.(c.it);
  box.innerHTML = `
    <div class="ibar"><button class="act" id="backToList">← Issues</button>
      <select id="iStatus" title="Status">${Object.entries(STATUS).map(([k, v]) => `<option value="${k}" ${k === i.status ? "selected" : ""}>${v}</option>`).join("")}</select>
      <select id="iPriority" title="Priority">${[1, 2, 3].map((p) => `<option value="${PRIORITY[p]}" ${p === i.priority ? "selected" : ""}>${PRIORITY[p]}</option>`).join("")}</select>
      <select id="iAssignee" title="Whose it is"><option value="">nobody's</option>${st.participants.map((p) => `<option value="${esc(p.id)}" ${p.id === i.assignee ? "selected" : ""}>${esc(p.name)}</option>`).join("")}</select>
    </div>
    <div class="ihead"><b>#${i.id}</b> <span id="iTitle" title="Click to rename">${esc(i.title)}</span>
      <div class="muted">${i.category} · raised by ${esc(workspace.participant(i.by).name)} ${when(i.time)}${i.place ? ` · <a href="#" id="iGo" title="Go there (Back returns)">${esc(pinSaid(i.place))}</a>` : " · not pinned"}${i.status === "waiting" ? ` · <b>waiting on ${esc(waitingOn(i))}</b>` : ""}</div></div>
    <div class="thread">${i.entries.map((e, k) => entryHtml(i, e, k)).join("") || `<div class="muted" style="padding:8px 2px">Nothing said yet.</div>`}</div>
    <div class="composer"><textarea id="reply" rows="2" placeholder="Reply in #${i.id}… (Enter sends; @name to call someone)"></textarea>
      <div class="row"><label class="c" title="Attach the view last used (${esc(describeView())}) as a link others can follow"><input type="checkbox" id="attach" ${attach ? "checked" : ""}> attach my view</label>${referTo ? `<button data-ref="${esc(referTo)}" title="Refer to what you have chosen: its id is put in your reply, and becomes a link">+ ${esc(c.of.title(c.it))}</button>` : ""}<button class="act primary" id="send">Send</button></div></div>`;
  const reply = $("reply");
  reply.value = draft;
  if (caret) { reply.focus(); reply.setSelectionRange(...caret); }
  for (const [id, html] of openDiffs) { const p = $(id); if (p) { p.hidden = false; p.innerHTML = html; } }
  const send = async () => {
    const text = reply.value.trim();
    if (!text) return;
    const args = { action: "comment", id: i.id, text };
    if ($("attach").checked && Object.keys(views).length) { args.view = linkOfView(); args.view_label = describeView(); }
    reply.value = "";
    $("attach").checked = false;
    const r = await workspace.call("issues", args);
    if (r.error) alert(r.error);
  };
  reply.onkeydown = (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } e.stopPropagation(); };
  $("send").onclick = send;
  $("backToList").onclick = () => { sel.issue = null; renderIssues(); rebuildMarks(); rebuildModel(); renderBoard(); renderTree(); syncHighlights(); };
  const update = (args) => workspace.call("issues", { action: "update", id: i.id, ...args }).then((r) => r.error && alert(r.error));
  $("iStatus").onchange = (e) => update({ status: e.target.value });
  $("iPriority").onchange = (e) => update({ priority: e.target.value });
  $("iAssignee").onchange = (e) => update({ assignee: e.target.value || null });
  // The title is edited in place: Enter keeps it, Esc puts it back.
  const title = $("iTitle");
  title.contentEditable = "plaintext-only";
  title.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); title.blur(); }
    if (e.key === "Escape") { title.textContent = i.title; title.blur(); }
  };
  title.onblur = () => { const t = title.textContent.trim(); if (t && t !== i.title) update({ title: t }); else title.textContent = i.title; };
  const go = $("iGo");
  // (To the thing it is pinned to, where that is a slab or a polyline, which are gone to their own way; else to its place.)
  if (go) go.onclick = (e) => { e.preventDefault(); const on = pinOf(i.place); on && (on.kind === "slab" || on.kind === "polyline" || on.kind === "photo") ? gotoObject(on.kind, on.id) : gotoIssue(i.id); };
  for (const a of box.querySelectorAll("a[data-objref]")) a.onclick = (e) => { e.preventDefault(); gotoObject(...a.dataset.objref.split(":")); };
  for (const b of box.querySelectorAll("button[data-view]")) b.onclick = () => followLink(i.entries[Number(b.dataset.view)].view);
  for (const a of box.querySelectorAll("a[data-selref]")) a.onclick = (e) => { e.preventDefault(); gotoSelection(a.dataset.selref); };
  for (const a of box.querySelectorAll("a[data-planeref]")) a.onclick = (e) => { e.preventDefault(); gotoPlane(a.dataset.planeref); };
  for (const b of box.querySelectorAll("button[data-ref]")) b.onclick = () => { const at = reply.selectionStart ?? reply.value.length; reply.setRangeText(`${at && !/\s$/.test(reply.value.slice(0, at)) ? " " : ""}${b.dataset.ref} `, at, reply.selectionEnd ?? at, "end"); reply.focus(); };
  // A member named in the thread: selected (it is highlighted, and the Member tab is its) and gone to.
  for (const a of box.querySelectorAll("a[data-member]")) a.onclick = (e) => { e.preventDefault(); choose("member", a.dataset.member); gotoMembers(a.dataset.member); };
  for (const a of box.querySelectorAll("a[data-issue]")) a.onclick = (e) => { e.preventDefault(); openIssue(Number(a.dataset.issue)); };
  for (const b of box.querySelectorAll("button[data-p]")) b.onclick = async () => {
    const p = i.entries.find((e) => e.kind === "proposal" && e.n === Number(b.dataset.p)), a = b.dataset.a;
    if (a === "show") return selectProposal(sel.proposal?.commit === p.commit ? null : { issue: i.id, n: p.n, commit: p.commit, mode: "changed" });
    if (a === "diff") {
      const pre = $(`diff-${i.id}-${p.n}`);
      pre.hidden = !pre.hidden;
      if (!pre.hidden && !pre.innerHTML) pre.innerHTML = diffHtml((await workspace.call("proposal", { issue: i.id, n: p.n, action: "diff" })).text);
      return;
    }
    return actOnProposal({ issue: i.id, n: p.n, commit: p.commit }, a);
  };
  // Something new in the thread: show it.
  const seen = `${i.id}:${i.entries.length}`;
  if (box.dataset.seen !== seen) { box.dataset.seen = seen; box.parentElement.scrollTop = box.parentElement.scrollHeight; }
}

export function renderBoard() {
  const box = $("boardPane");
  if (!box || !st) return;
  if (!shared()) { box.innerHTML = `<div class="muted" style="padding:12px">Issues are raised and worked in a workspace kept by a service (topo serve, of a project that is a git repository of its own): this project's is kept in its own files, by this page alone.</div>`; return setInfo("board:issues", ""); }
  const cols = ["waiting", "doing", "open", "done"];
  const list = ordered().filter((i) => sel.filter === "all" || i.category === sel.filter);
  box.innerHTML = cols.map((c) => {
    const cards = list.filter((i) => i.status === c);
    return `<div class="col" data-col="${c}"><h4>${STATUS[c]} <span class="muted">${cards.length}</span></h4><div class="cards">${cards.map((i) => `
      <div class="card p${i.priority}${i.id === sel.issue ? " on" : ""}${onMe(i) ? " mine" : ""}" draggable="true" data-id="${i.id}">
        <div><b>#${i.id}</b> ${esc(i.title)}</div>
        <div class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="cat">${i.category}</span>${i.entries.length ? `<span class="muted">${i.entries.length}</span>` : ""}${i.status === "waiting" ? `<span class="muted">on ${esc(waitingOn(i))}</span>` : ""}</div>
      </div>`).join("")}</div></div>`;
  }).join("");
  for (const card of box.querySelectorAll(".card")) {
    card.onclick = () => openIssue(Number(card.dataset.id));
    card.ondragstart = (e) => { e.dataTransfer.setData("text/plain", card.dataset.id); e.dataTransfer.effectAllowed = "move"; card.classList.add("drag"); };
    card.ondragend = () => card.classList.remove("drag");
  }
  for (const col of box.querySelectorAll(".col")) {
    col.ondragover = (e) => { e.preventDefault(); col.classList.add("over"); };
    col.ondragleave = () => col.classList.remove("over");
    col.ondrop = async (e) => {
      e.preventDefault();
      col.classList.remove("over");
      const id = Number(e.dataTransfer.getData("text/plain")), status = col.dataset.col;
      // Where in the column: before the card under the cursor. Its priority and a rank just above it.
      const cards = [...col.querySelectorAll(".card")].filter((c) => Number(c.dataset.id) !== id);
      const below = cards.find((c) => e.clientY < c.getBoundingClientRect().top + c.offsetHeight / 2);
      const above = below ? cards[cards.indexOf(below) - 1] : cards[cards.length - 1];
      const iss = (c) => c && st.issues.find((i) => i.id === Number(c.dataset.id));
      const [a, b] = [iss(above), iss(below)];
      const args = { action: "update", id, status };
      if (a || b) {
        args.priority = PRIORITY[(b || a).priority];
        args.rank = a && b && a.priority === b.priority ? 0.5 * (a.rank + b.rank) : b ? b.rank - 1 : a.rank + 1;
      }
      const r = await workspace.call("issues", args);
      if (r.error) alert(r.error);
    };
  }
  const n = (c) => st.issues.filter((i) => i.status === c).length;
  setInfo("board:issues", `waiting ${n("waiting")} · in hand ${n("doing")} · open ${n("open")} · done ${n("done")}`);
}

// What an issue is to the tree, the menus and the views (kinds.js). (It is opened, not chosen: `sel.issue`.)
const PIN = { 1: "#d62828", 2: "#1f5fbf", 3: "#6b6b66" };
kind("issue", {
  group: "issues", order: 20,
  key: (id) => `issue:${id}`,
  find: (id) => st?.issues.find((i) => i.id === Number(id)) ?? null,
  choose: (id) => openIssue(Number(id)),
  goto: (id) => gotoIssue(Number(id)),
  // Each with what was made for it under it (homes.js): folded away, till it is the one open.
  rows: (row, list) => (shared() ? list("issues", "Issues", () => ordered().flatMap((i) => {
    const things = countOf(i.id), unfolded = things && (objects.open.has(`issue:${i.id}`) || i.id === sel.issue);
    return [row(1, "issue", `issue:${i.id}`, `#${i.id} ${i.title}`, { id: i.id, group: !!things, unfolded, on: i.id === sel.issue && !chosen(), note: things || (i.place ? "" : "not pinned"), dot: i.status === "done" ? "unseen" : i.status === "waiting" ? "contradicted" : "" }), ...(unfolded ? rowsOf(i.id, row, 2) : [])];
  }), { note: st.issues.filter((i) => i.status !== "done").length }) : ""),
  menu(id, m) {
    const i = this.find(id);
    m.add("Open it", () => openIssue(i.id));
    if (i.place) m.add("Go to it", () => gotoIssue(i.id));
    m.add(i.status === "done" ? "Reopen it" : "Mark it done", () => workspace.call("issues", { action: "update", id: i.id, status: i.status === "done" ? "open" : "done" }));
    m.showHide();
    if (countOf(i.id)) m.add(keptInSight(i.id) ? "Draw its things only while it is open" : "Keep its things drawn while it is not open", () => keepInSight(i.id, !keptInSight(i.id)));
    m.divider();
    m.add("Delete it", () => { if (confirm(`Delete issue #${i.id}, its thread${countOf(i.id) ? ` and the ${countOf(i.id)} things made for it` : ""}?\n\n${i.title}`)) workspace.call("issues", { action: "delete", id: i.id }); }, "danger");
  },
  // The one open: the box of its place. All that are pinned (and not done): pins.
  marks(out) {
    const open = sel.issue != null && this.find(sel.issue);
    if (open?.place && vis("issues", `issue:${open.id}`)) out.box(corners(open.place), [255, 255, 255]);
    for (const i of st.issues) {
      if (!i.place || !vis("issues", `issue:${i.id}`) || (i.status === "done" && i.id !== sel.issue)) continue;
      out.pin({ id: i.id, pos: middle(i.place), text: `#${i.id} ${i.title.length > 26 ? i.title.slice(0, 25) + "…" : i.title}`, title: `${i.title} — ${STATUS[i.status]}`, colour: PIN[i.priority], on: i.id === sel.issue, dim: i.status === "done" });
    }
  },
});
