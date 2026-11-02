// Starting and joining a session, and what the rest of the page is told of one. A page is of one project, or of a session on one
// (its address says: `#room=id.secret`, which a browser does not send anywhere). None of this is loaded for a page that is not.

import { msg, say as sayStatus } from "../shell/panel.js";
import { RELAYED, engineStamp, project, storage } from "../project.js";
import { workspace } from "../workspace.js";
import { auth } from "../auth.js";
import { live, renderEditors } from "../editor.js";
import { newRoom } from "../room.js";
import { GITHUB_APP } from "../config.js";
import { BranchStore, githubPlace } from "../store.js";
import { GuestRepo, HostRepo } from "../host-git.js";
import { Session } from "./session.js";
import { SessionWork } from "./work.js";
import { merger } from "../work.js";
import { shareMenu } from "../share-menu.js";

/** How long a room is given to say what it is, before the link is taken to be no good. */
const OPENED_WITHIN = 20000;
/** Files arriving together build the model once. */
const BUILD_AFTER = 30;

const relay = () => (auth.app?.broker ?? GITHUB_APP.broker).replace(/^http/, "ws");
/** What keeps the rooms of sessions on a location: a host, its own; anything to be reached from elsewhere, the relay. */
const roomsOf = (kind) => (githubPlace(kind) || kind === RELAYED ? relay() : new URL("./", location.href).href.replace(/\/$/, ""));

/** Asks the host this page is served by to be in a room, or to leave one (crates/topo-cli/src/share.rs): its id. */
async function toHost(body) {
  const r = await fetch("share", { method: "POST", body: JSON.stringify(body) });
  const j = await r.json();
  if (j.error) throw new Error(j.error);
  return j.host;
}
/** The id of the host this page is served by; none: it is not. */
const hostHere = () => fetch("share").then((r) => (r.ok ? r.json() : null)).then((j) => j?.host ?? null, () => null);
const shareRoom = (room, entry) => toHost({ relay: relay(), origin: location.origin, room: room.id, secret: room.secret, entry });
/** Who this person is to others: the name they gave, or who they are signed in as, and that one's picture. */
function me() {
  const login = auth.signedIn?.() ? auth.login() : null;
  return { who: workspace.id, name: workspace.myName() || login || `Guest ${workspace.id.slice(-3)}`, picture: login ? `https://avatars.githubusercontent.com/${encodeURIComponent(login)}?s=88` : null };
}
/** The location a session is of, as the page's address says it: the address is what a link is. */
const kindInAddress = () => new URLSearchParams(location.search).get("in") ?? "host";

/** The room the page's address names: `{ id, secret }`, or null. */
export function roomInAddress(hash = location.hash) {
  const m = /^#room=([A-Za-z0-9_-]+)\.([A-Za-z0-9_-]+)$/.exec(hash);
  return m && { id: m[1], secret: m[2] };
}
/** The address a session is joined at. */
const linkTo = ({ id, secret }, entry, kind) => `${project.href(entry, kind)}#room=${id}.${secret}`;

/** Three ways, by what a pull merges by: loaded first, since a page in a session merges everything that comes. */
async function mergeOfFiles() {
  const lib = await merger();
  return (file, base, ours, theirs) => lib.merge_file(file, base, ours, theirs);
}

/** Waits till `f()`, or the time is up. */
async function until(f, ms) {
  const end = Date.now() + ms;
  while (!f()) {
    if (Date.now() > end) return false;
    await new Promise((r) => setTimeout(r, 25));
  }
  return true;
}

/**
 * A session begun on the project open, at the commit it is at, with this person's changes going in: the page then goes to it.
 * (Their own changes of the branch stay where they were, for when they leave.) `overInternet`: its room on the relay, with the host there too,
 * for someone anywhere to join on the published app.
 */
export async function startSession({ overInternet = false } = {}) {
  const work = storage.work, entry = project.current(), kind = overInternet ? RELAYED : storage.kind, action = overInternet ? "share" : "session";
  if (!work?.can(action) || !entry) throw new Error(work?.whyNot(action) ?? "A session is of a project that has versions, so that everyone has the same files.");
  if (!crypto.subtle) throw new Error("A session needs a secure page (https, or localhost: an ssh tunnel to this host will do).");
  await work.status();
  const room = newRoom(), { git, branch, commit, name } = work.sessionFrom;
  const session = new Session({ relay: roomsOf(kind), ...room, me: me(), create: true, merge: await mergeOfFiles(), read: (at, file) => git.textAt(at, file) });
  await until(() => session.status === "live", OPENED_WITHIN) || (() => { throw new Error("What keeps sessions could not be reached, so none could be started."); })();
  const host = overInternet ? await shareRoom(room, entry) : undefined;
  await session.say({ k: "open", place: kind, name, branch, entry, commit, engine: await engineStamp(), host });
  await session.say({ k: "hello", ...me() });
  await until(() => !!session.opened, OPENED_WITHIN);
  for (const e of await work.entries()) await session.edit(e.id, e.gone ? null : e.text, { build: true });
  await until(() => session.settled, OPENED_WITHIN);
  session.close();
  await project.leave();
  const link = linkTo(room, entry, kind), same = new URL(link).search === location.search;
  location.assign(link);
  // (The address differs only after its #: that alone loads nothing.)
  if (same) location.reload();
}

/** The session that is open on this page. */
export let current = null;

/**
 * Joins the room the address names, before the project is opened: `{ kind, entry }`, what the page is then of, or throws why not.
 * The log is read through first, so that the project opens as it now is.
 */
export async function joinRoom(room) {
  let store = null, kept = false, building = 0, cannot = null;
  const build = () => { clearTimeout(building); building = setTimeout(() => project.touched(), BUILD_AFTER); };
  const merge = await mergeOfFiles();
  const session = new Session({
    relay: roomsOf(kindInAddress()), ...room, me: me(), merge,
    read: async (commit, file) => {
      // (What cannot be read is found out once: each of the log's files is not waited for in turn.)
      if (cannot) throw cannot;
      try {
        return await store.base.git.textAt(commit, file);
      } catch (e) {
        cannot = e;
        throw e;
      }
    },
    on: {
      open: async (of) => {
        const place = `session:${room.id}`, onBranch = githubPlace(of.place);
        // (On the host it is shared from, as anywhere else on a host, the host is asked directly. From elsewhere, by the room.)
        const guest = !!of.host && (await hostHere()) !== of.host;
        const git = guest ? new GuestRepo({ entry: of.entry, askHost: (action, args) => session.askHost(action, args) }) : new HostRepo({ entry: of.entry, who: () => session.me.name });
        const base = onBranch ? storage.branchStore(onBranch, { place }) : new BranchStore({ git, repo: of.name, branch: of.branch, place });
        store = storage.sessionOn(of.place, room.id, base, (b, top) => new SessionWork(b, top, session));
        return store.base.moveTo(of.commit).catch((e) => { cannot = e; });
      },
      base: (commit) => { if (kept) store.moved(commit).then(build); },
      changed: (file, c) => {
        if (!kept) return;
        if (c.live !== c.liveWas) live.changed?.(file, c.live);
        if (c.built !== c.builtWas) store.arrived(file).then(build);
        if (c.dropped && c.own) {
          msg(`Your edit to ${file} was not kept: someone else changed those lines first. It is in Changes, to compare and put back.`, true);
          store.notKept(file, { mine: c.mine, now: c.live, base: c.base });
        }
      },
      refused: (file, why, text, back) => {
        if (why === "clash") {
          msg(`Your edit to ${file} was not kept: someone else changed those lines first. It is in Changes, to compare and put back.`, true);
          if (kept) store.notKept(file, { mine: back.mine, now: text, base: back.base });
          return;
        }
        msg(`Your edit to ${file} could not be shared: ${why === "large" ? "it is too large for one entry" : "the session is full"}.`, true);
        if (kept) { live.changed?.(file, text); store.arrived(file).then(build); }
      },
      diverged: (file) => msg(`${file} is not the same here as for someone else in this session, so one of you is seeing the wrong text: you may be on different builds of the app. Reload, and compare before anyone commits.`, true),
      people: () => { if (kept) { workspace.refresh(); renderEditors(); } },
      chat: () => { if (kept) workspace.refresh(); },
      cannot: ({ name, why }) => msg(`${name} joined but cannot see this project: ${why}`, true),
      status: (s) => showStatus(s),
      gone: () => msg("The relay no longer has this session.", true),
      ended: () => { if (kept) msg("Whoever shared this session has stopped sharing it: what is in it can no longer be read from their host.", true); },
    },
  });
  if (!(await until(() => session.status === "live" && session.opened, OPENED_WITHIN))) { session.close(); throw new Error("no such session: it may have ended, or the link may be incomplete"); }
  await session.caughtUpOnce;
  if (session.ended) { session.close(); throw new Error("whoever shared it has stopped sharing it"); }
  if (cannot) {
    session.say({ k: "cannot", who: session.me.who, name: session.me.name, why: store.base.git.unreadable(cannot) });
    throw new Error(`you cannot read ${session.opened.name}. Those who invited you have been told. ${auth.signedIn?.() || !githubPlace(session.opened.place) ? "" : "Sign in, then open the link again."}`);
  }
  current = session;
  // (Its host, started again since, is told to go back: this is the page of whoever shared it.)
  if (session.opened.host && !(store.base.git instanceof GuestRepo)) await shareRoom(room, session.opened.entry);
  // (What is kept here is made as the log says: it, not this browser, is the truth.)
  for (const file of session.replica.touched) await store.arrived(file);
  kept = true;
  wire(session, store);
  return { kind: session.opened.place, entry: session.opened.entry };
}

function showStatus(s) {
  sayStatus("session", s === "live" ? null : "reconnecting to the session…");
}

/** What the page hears of the session, and how it speaks to it. */
function wire(session, store) {
  workspace.together = {
    state: () => ({ participants: session.people(), chat: session.messages }),
    call(tool, args) {
      if (tool === "chat" && args.action === "send") return session.say({ k: "say", who: session.me.who, to: args.to, text: args.text }).then(() => ({ text: "sent", images: [] }));
      if (tool === "rename") {
        workspace.setMyName(args.name);
        session.rename(args.name);
        return Promise.resolve({ text: `you are ${args.name}`, images: [] });
      }
      return null;
    },
  };
  // Typing goes as it is typed, at a pause; it is not built from till it is saved.
  live.text = (file) => session.replica.has(file) ? session.replica.live(file) : null;
  live.typed = (file, text) => session.edit(file, text, { build: false });
  live.over = (file, from, to, onto) => session.replica.merge(file, from, onto, to) ?? onto;
  live.caret = (file, line) => session.present({ caret: file ? { file, line } : null });
  live.lines = (file) => {
    const out = new Map();
    for (const said of session.here.values()) {
      if (said.caret?.file !== file) continue;
      const p = session.participants.get(said.who);
      if (p) out.set(said.caret.line, [...(out.get(said.caret.line) ?? []), p]);
    }
    return out;
  };
  // (The scene's part of it, with what is drawn of others and how they are followed.)
  import("../scene/together.js").then(({ attach }) => attach(session));
  workspace.refresh();
  inviteButton(session, store);
  warnOfBuild(session.opened.engine);
  store.said();
}

/** Whether the session was begun on another build of the app, which said so: it may not read or solve the model as this one does. */
export async function warnOfBuild(engine) {
  if (!engine || engine === (await engineStamp())) return false;
  msg("This session was started on a different build of the app, so a model may not build or solve here as it does for them. Reload to get the latest.", true);
  return true;
}

/** The address someone is asked into the session at: where it is shared over the internet, on the published app. */
export function inviteLink(session = current) {
  const { entry, place: kind } = session.opened, room = { id: session.id, secret: session.secret };
  if (kind !== RELAYED) return linkTo(room, entry, kind);
  const u = new URL(auth.app?.site ?? GITHUB_APP.site);
  u.search = `?in=${encodeURIComponent(kind)}&project=${encodeURIComponent(entry)}`;
  u.hash = `room=${room.id}.${room.secret}`;
  return u.href;
}

/** Whoever shared it stops: it ends for everyone, and the host leaves the room. */
export async function stopSharing(session = current) {
  await session.say({ k: "ended" });
  await toHost({ stop: session.id });
}

/** The header's Share menu, in a session: how others are asked in, and how to leave. */
function inviteButton(session, store) {
  const { entry, place: kind, name } = session.opened, link = inviteLink(session), sharer = kind === RELAYED && !(store.base.git instanceof GuestRepo);
  const leave = () => (kind !== RELAYED ? project.go(entry, kind) : sharer ? project.go(entry, "host") : location.assign(project.href(null)));
  shareMenu({
    label: "In a session ▾",
    title: "Who is in this session is shown beside this. Ask someone in, or leave",
    items: () => [
      { label: "Copy invite link", run: () => navigator.clipboard.writeText(link).then(() => msg("Link copied. Whoever opens it can read and change this session, so send it only to those you mean to."), () => msg(link)) },
      { label: "Invite by email…", run: () => { location.href = `mailto:?subject=${encodeURIComponent(`Join me on ${entry}`)}&body=${encodeURIComponent(`A topo-cad session on ${name}:\n\n${link}\n\n${store.base.git.joining}`)}`; } },
      ...(sharer ? [{ label: "Stop sharing…", run: async () => { if (confirm("End this session for everyone? What has not been committed goes with it.")) { await stopSharing(session); leave(); } } }] : []),
      { label: "Leave the session", run: leave },
    ],
  });
}
