// Who is in a shared workspace, or a session on one: their badges in the header, those online and
// those with something unread, and a chat with one of them (outside the issues). Messages are the
// workspace's; what has been read is this browser's (a bell on a badge: something unread from them).
// In a session, a click on someone goes to their view (together.js).

import { $, el, esc, store, stored } from "../ui.js";
import { workspace } from "../workspace.js";
import { sel, st } from "./state.js";
import { bindRefs, refs, when } from "./issues.js";

/** What a session offers of someone, where the page is in one: `follow(who)`, whether it could; `chat(who)`; `menuFor(who, event, chat)`. */
export const personActions = { follow: null, chat: null, menuFor: null, isFollowed: null };

export function badge(id, small) {
  const p = workspace.participant(id);
  const initials = p.kind === "agent" ? (p.name.match(/[A-Za-z]/)?.[0] ?? "A").toUpperCase() + (p.name.match(/\d+/)?.[0] ?? "") : p.name.slice(0, 1).toUpperCase();
  // An agent's badge has a little robot's head in it, so it is not taken for a person.
  return `<span class="who${small ? " small" : ""}${p.kind === "agent" ? " agent" : ""}" style="--who:${esc(p.colour)}${p.picture ? `;background-image:url(${esc(p.picture)});background-size:cover;color:transparent` : ""}" title="${esc(p.name)}${p.kind === "agent" ? " (agent)" : ""}">${p.kind === "agent" ? BOT : ""}${esc(initials)}</span>`;
}
const BOT = `<svg class="bot" viewBox="0 0 12 12" width="11" height="11"><path d="M6 .8v1.7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><rect x="1.5" y="2.8" width="9" height="7.4" rx="2" fill="currentColor"/><circle cx="4.2" cy="6.3" r="1.1" fill="var(--who)"/><circle cx="7.8" cy="6.3" r="1.1" fill="var(--who)"/></svg>`;
const BELL = `<svg viewBox="0 0 12 12" width="11" height="11"><path d="M6 1.2c-2 0-3.2 1.5-3.2 3.4 0 2-.6 3-1.3 3.6h9c-.7-.6-1.3-1.600-1.300-3.600 0-1.900-1.200-3.400-3.200-3.400z M4.800 9.300a1.250 1.250 0 0 0 2.400 0" fill="currentColor"/></svg>`;

// A chat: opened from a badge in the header.
const chatRead = stored("topo.chat.read", {});
const chatWith = (peer) => (st?.chat || []).filter((m) => (m.from === peer && m.to === workspace.id) || (m.from === workspace.id && m.to === peer));
const unreadFrom = (peer) => (st?.chat || []).filter((m) => m.from === peer && m.to === workspace.id && m.time > (chatRead[peer] ?? 0)).length;

personActions.chat = (peer) => openChat(peer);
function openChat(peer) {
  sel.chat = sel.chat === peer ? null : peer;
  renderChat(true);
  renderPeople();
}

export function renderChat(focus) {
  let box = $("chatPanel");
  const peer = sel.chat && st?.participants.find((p) => p.id === sel.chat);
  if (!peer) { box?.remove(); return; }
  if (!box) {
    box = el("div", { id: "chatPanel" }, `<div class="chead"><span class="cwho"></span><button class="cx" title="Close">×</button></div><div class="cnote muted"></div><div class="clist"></div><textarea rows="2"></textarea>`);
    document.body.appendChild(box);
    box.querySelector(".cx").onclick = () => { sel.chat = null; renderChat(); };
    const ta = box.querySelector("textarea");
    ta.onkeydown = async (e) => {
      e.stopPropagation();
      if (e.key === "Escape") { sel.chat = null; return renderChat(); }
      if (e.key !== "Enter" || e.shiftKey) return;
      e.preventDefault();
      const text = ta.value.trim();
      if (!text) return;
      ta.value = "";
      const r = await workspace.call("chat", { action: "send", to: sel.chat, text });
      if (r.error) { alert(r.error); ta.value = text; }
    };
  }
  const agent = peer.kind === "agent";
  box.querySelector(".cwho").innerHTML = `${badge(peer.id)} <b>${esc(peer.name)}</b> <span class="muted">${agent ? "agent · " : ""}${PRESENCE[peer.presence]}</span>`;
  box.querySelector(".cnote").textContent = agent && peer.presence === "away" ? `${peer.name} is not listening just now: it gets this when it next is.` : !agent && peer.presence === "away" ? `${peer.name} is away: they see this when they are back.` : "";
  box.querySelector("textarea").placeholder = `Say something to ${peer.name}… (Enter sends)`;
  const list = box.querySelector(".clist"), msgs = chatWith(peer.id), key = `${peer.id}:${msgs.length}`;
  if (list.dataset.key !== key) {
    list.dataset.key = key;
    list.innerHTML = msgs.map((m) => `<div class="cmsg ${m.from === workspace.id ? "mine" : "theirs"}"><div class="text">${refs(m.text)}</div><span class="muted">${when(m.time)}</span></div>`).join("") ||
      `<div class="muted" style="padding:6px 2px">Nothing said yet. ${agent ? "Ask it something, or what it is doing. For something about a place in the building, raise an issue: it will see that too." : ""}</div>`;
    list.scrollTop = list.scrollHeight;
    bindRefs(list);
  }
  // Seen, now that it is open.
  const last = msgs.filter((m) => m.from === peer.id).reduce((a, m) => Math.max(a, m.time), 0);
  if (last > (chatRead[peer.id] ?? 0)) { chatRead[peer.id] = last; store("topo.chat.read", chatRead); }
  if (focus) box.querySelector("textarea").focus();
}
const PRESENCE = { here: "here", working: "working", waiting: "listening", away: "away" };
export function renderPeople() {
  const box = $("people");
  box.hidden = !workspace.can("people");
  // (Online; or not, with something from them not yet read.)
  const listed = st.participants.filter((p) => p.id === workspace.id || p.presence !== "away" || unreadFrom(p.id));
  box.innerHTML = listed.map((p) => {
    const me = p.id === workspace.id, n = me ? 0 : unreadFrom(p.id), followable = !me && p.presence !== "away" && personActions.follow;
    return `<span class="person ${p.presence}${me ? " me" : ""}${sel.chat === p.id ? " chatting" : ""}${personActions.isFollowed?.(p.id) ? " followed" : ""}" data-id="${esc(p.id)}" title="${esc(p.name)} — ${p.kind === "agent" ? "agent, " : ""}${PRESENCE[p.presence]}${me ? " (you: click to change your name)" : followable ? `. Click to follow their view; right-click for more${n ? ` (${n} unread)` : ""}` : `. Click to chat${n ? ` (${n} unread)` : ""}`}">${badge(p.id)}<i></i>${n ? `<b class="bell" title="${n} unread">${BELL}</b>` : ""}</span>`;
  }).join("");
  for (const s of box.querySelectorAll(".person:not(.me)")) {
    const id = s.dataset.id;
    // (The bell, or anyone offline: what they said.)
    s.onclick = (e) => { if (e.target.closest(".bell") || !personActions.follow?.(id)) openChat(id); };
    s.oncontextmenu = (e) => { if (!personActions.menuFor) return; e.preventDefault(); personActions.menuFor(id, e, () => openChat(id)); };
  }
  const me = box.querySelector(".me");
  if (me) me.onclick = async () => {
    const name = prompt("Your name, as others see it:", workspace.me()?.name ?? "");
    if (name && name.trim()) await workspace.call("rename", { name: name.trim() });
  };
}
