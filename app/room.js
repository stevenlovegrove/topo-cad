// A room, as a page has it: what is said is encrypted here with a key whatever keeps the room never sees (it is in the fragment of the
// invite link, which a browser does not send), given in the room's order, and sent again after a connection goes, kept once.
// docs/collaboration.md says what a session is; session/session.js is its use.
//
// What keeps a room says the same things either way, and is reached by its address:
//
//   a relay   relay/src/relay.js: `wss://…`, by a WebSocket
//   a host    crates/topo-cli/src/rooms.rs, a `topo serve`: `http://…`, by a stream one way and requests the other

/** How often the connection is asked after: often enough to see one gone, and to outlast a network that drops quiet ones. */
const PING_EVERY = 30000;
/** How long a host's stream may say nothing (it says something every second) before it is taken to have gone. */
const SILENT_AT_MOST = 6000;
/** How long to wait before connecting again, at the longest. */
const RETRY_AT_MOST = 30000;

const url64 = (bytes) => btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
const bytes64 = (text) => Uint8Array.from(atob(text.replaceAll("-", "+").replaceAll("_", "/")), (c) => c.charCodeAt(0));
const keyFrom = (raw) => crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]);

async function seal(key, value) {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, new TextEncoder().encode(JSON.stringify(value))));
  return url64(iv) + "." + url64(sealed);
}
async function open(key, body) {
  const [iv, sealed] = body.split(".").map(bytes64);
  return JSON.parse(new TextDecoder().decode(await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, sealed)));
}

/** A new room: `{ id, secret }`, which an invite link carries as `#room=id.secret`. */
export function newRoom() {
  return { id: url64(crypto.getRandomValues(new Uint8Array(16))), secret: url64(crypto.getRandomValues(new Uint8Array(32))) };
}

/** A connection to a relay: `{ send(m), open, close() }`. `on`: `message(m)`; `closed()`, once. */
function socketLink(url, after, on) {
  const ws = new WebSocket(`${url}?after=${after}`);
  let answered = true, gone = false;
  const ping = setInterval(() => {
    if (!answered) return ws.close();
    answered = false;
    if (ws.readyState === WebSocket.OPEN) ws.send("ping");
  }, PING_EVERY);
  ws.onmessage = (e) => {
    answered = true;
    if (e.data !== "pong") on.message(JSON.parse(e.data));
  };
  // (Both: a connection that was never made says only `error` in some runtimes, and in browsers says both.)
  ws.onclose = ws.onerror = () => {
    if (gone) return;
    gone = true;
    clearInterval(ping);
    on.closed();
  };
  return {
    send: (m) => ws.send(JSON.stringify(m)),
    get open() { return ws.readyState === WebSocket.OPEN; },
    close: () => ws.close(),
  };
}

/** The same of a host: lines as they come, and what is sent as requests, in the order said (what is said while one goes, all in the next). */
function streamLink(url, after, on) {
  const stop = new AbortController();
  let peer = null, gone = false, heard = Date.now(), queue = [], sending = false;
  const closed = () => {
    if (gone) return;
    gone = true;
    clearInterval(watch);
    stop.abort();
    on.closed();
  };
  const watch = setInterval(() => { if (Date.now() - heard > SILENT_AT_MOST) closed(); }, SILENT_AT_MOST / 3);
  const pump = async () => {
    if (sending) return;
    sending = true;
    while (queue.length && !gone) {
      const body = JSON.stringify(queue);
      queue = [];
      // (Lost with the connection: what must arrive is sent again when it is back.)
      await fetch(`${url}?peer=${encodeURIComponent(peer)}`, { method: "POST", body, signal: stop.signal }).catch(() => {});
    }
    sending = false;
  };
  (async () => {
    const r = await fetch(`${url}?after=${after}`, { signal: stop.signal, cache: "no-store" });
    if (!r.ok) throw new Error(`${r.status}`);
    const lines = r.body.pipeThrough(new TextDecoderStream()).getReader();
    let held = "";
    for (;;) {
      const { value, done } = await lines.read();
      if (done) break;
      heard = Date.now();
      const all = (held + value).split("\n");
      held = all.pop();
      for (const line of all) {
        if (!line) continue;
        const m = JSON.parse(line);
        if (m.t === "welcome") peer = m.peer;
        on.message(m);
      }
    }
  })().catch(() => {}).finally(closed);
  return {
    send: (m) => { queue.push(m); pump(); },
    get open() { return !!peer && !gone; },
    close: closed,
  };
}

export class Room {
  /** `relay`: what keeps it (`wss://…`, a relay; `http://…`, a host). `after`: the last entry already had, for a page coming back to a room.
   *  `on`: `entry({ seq, id, from, at, value })`, in order, each once; `peers(list)`; `signal(from, value)`;
   *  `refused(id, why)`; `status("connecting" | "live")`; `caughtUp()`: all there was has been told, each time it connects;
   *  `forgotten()`: the relay no longer has what is had here;
   *  `unreadable({ seq, id, from, at })`: an entry this key does not open (the wrong link, or not from anyone with one). */
  constructor({ relay, id, secret, after = 0, on = {} }) {
    this.url = `${relay}/rooms/${id}`;
    this.key = keyFrom(bytes64(secret));
    this.seq = after;
    this.on = on;
    /** What has been sent and not yet seen in the log: id → body. */
    this.waiting = new Map();
    /** Told in order, though opening each takes a while. */
    this.opening = Promise.resolve();
    /** What peers say, told in order, though opening each takes a while: apart from the log, since what an entry means may be asked of a peer. */
    this.hearing = Promise.resolve();
    /** Sent in the order said, though sealing each takes a while. */
    this.saying = Promise.resolve();
    this.peer = null;
    this.peers = new Set();
    this.tries = 0;
    this.closed = false;
    this.connect();
  }

  connect() {
    if (this.closed) return;
    this.on.status?.("connecting");
    const open = /^ws/.test(this.url) ? socketLink : streamLink;
    const link = (this.link = open(this.url, this.seq, {
      message: (m) => this.heard(m),
      closed: () => {
        if (this.link !== link || this.closed) return;
        this.on.status?.("connecting");
        // (Not all at once, when a restart has dropped everyone.)
        const wait = Math.min(RETRY_AT_MOST, 500 * 2 ** this.tries++) * (0.5 + Math.random() / 2);
        setTimeout(() => this.connect(), wait);
      },
    }));
  }

  heard(m) {
    if (m.t === "entry") {
      this.waiting.delete(m.id);
      this.inOrder(async () => {
        if (m.seq <= this.seq) return;
        this.seq = m.seq;
        const { body: _, ...entry } = m;
        let value;
        try {
          value = await open(await this.key, m.body);
        } catch (_) {
          return this.on.unreadable?.(entry);
        }
        // (The next waits for it: what an entry means may take reading.)
        await this.on.entry?.({ ...entry, value });
      });
    } else if (m.t === "welcome") {
      this.tries = 0;
      this.peer = m.peer;
      this.peers = new Set(m.peers);
      if (m.head < this.seq) this.on.forgotten?.();
      this.on.peers?.([...this.peers]);
      this.on.status?.("live");
      // (All that has reached the log has been seen by now: what has not been is sent again.)
      for (const [id, body] of this.waiting) this.link.send({ t: "entry", id, body });
      this.inOrder(() => this.on.caughtUp?.());
    } else if (m.t === "joined" || m.t === "left") {
      this.peers[m.t === "joined" ? "add" : "delete"](m.peer);
      this.on.peers?.([...this.peers]);
    } else if (m.t === "signal") {
      this.hearing = this.hearing.then(async () => this.on.signal?.(m.from, await open(await this.key, m.body))).catch((e) => console.error("room:", e));
    } else if (m.t === "refused") {
      this.waiting.delete(m.id);
      this.on.refused?.(m.id, m.why);
    }
  }
  inOrder(f) {
    this.opening = this.opening.then(f).catch((e) => console.error("room:", e));
    return this.opening;
  }

  /** An id for what is about to be said. */
  newId() {
    return url64(crypto.getRandomValues(new Uint8Array(12)));
  }
  /** Into the log, for everyone: comes back as an entry with its place, in the order said here. Its id (`id`: this one). */
  say(value, id = this.newId()) {
    this.saying = this.saying.then(async () => {
      const body = await seal(await this.key, value);
      this.waiting.set(id, body);
      if (this.link.open) this.link.send({ t: "entry", id, body });
    }).catch((e) => console.error("room:", e));
    return this.saying.then(() => id);
  }
  /** To one peer, now or not at all, after what was said here before it. (Its order matters: WebRTC's candidates are no use
   *  ahead of the offer they belong to.) */
  signal(to, value) {
    this.saying = this.saying.then(async () => {
      const body = await seal(await this.key, value);
      if (this.link.open) this.link.send({ t: "signal", to, body });
    }).catch((e) => console.error("room:", e));
    return this.saying;
  }
  close() {
    this.closed = true;
    this.link.close();
  }
}
