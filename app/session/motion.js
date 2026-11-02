// What is only for showing (docs/collaboration.md, "Seeing each other"): a data channel to each peer that keeps no order and
// sends nothing twice, so that what is late is lost rather than holding up what is newer. There is no way round: two peers that
// cannot connect are told so, and what they would have seen is simply not there. WebRTC's negotiation goes by the room's signals.

/** How long a connection is given before it is said to have failed. */
const CONNECT_WITHIN = 15000;
/** Where a peer learns how the internet sees it. No relay of data (TURN): a pair that needs one cannot connect, and that is said. */
const ICE = [{ urls: "stun:stun.cloudflare.com:3478" }];

export class Mesh {
  /** `signal(peer, value)`: to one peer, by the room. `on`: `packet(peer, value)`; `state(peer, "connecting" | "open" | "failed")`.
   *  `RTC`: what makes connections. */
  constructor({ signal, on = {}, RTC = globalThis.RTCPeerConnection }) {
    Object.assign(this, { signal, on, RTC });
    this.links = new Map();
    this.me = null;
  }
  /** Who is here now (by the relay's ids), and which of them this is. Each pair connects; those gone are let go of. */
  peersAre(me, list) {
    this.me = me;
    const now = new Set(list);
    for (const peer of [...this.links.keys()]) if (!now.has(peer)) this.drop(peer);
    for (const peer of list) if (peer !== me && !this.links.has(peer)) this.link(peer);
  }
  /** A peer's state now: "connecting", "open", "failed"; undefined: no such peer. */
  stateOf(peer) {
    return this.links.get(peer)?.state;
  }
  /** How each connection stands, for finding out why one has not been made. */
  report() {
    return [...this.links.values()].map((l) => ({ peer: l.peer, state: l.state, offers: l.offers, connection: l.pc.connectionState, ice: l.pc.iceConnectionState, signaling: l.pc.signalingState, channel: l.channel.readyState }));
  }
  /** Whether anyone would hear what is sent. */
  get anyone() {
    for (const l of this.links.values()) if (l.state === "open") return true;
    return false;
  }
  set(l, state) {
    if (l.state === state) return;
    l.state = state;
    clearTimeout(l.timer);
    this.on.state?.(l.peer, state);
  }
  link(peer) {
    const pc = new this.RTC({ iceServers: ICE });
    // (One of a pair offers, by their ids, so that offers never cross: a first offer rolled back to take the other's leaves
    // a data channel that never opens.)
    const l = { peer, pc, state: null, offers: this.me > peer, timer: null, channel: null };
    this.links.set(peer, l);
    this.set(l, "connecting");
    l.timer = setTimeout(() => this.set(l, "failed"), CONNECT_WITHIN);
    // (Negotiated: both make the one channel, and neither has to wait to be offered it.)
    l.channel = pc.createDataChannel("motion", { negotiated: true, id: 0, ordered: false, maxRetransmits: 0 });
    l.channel.onopen = () => this.set(l, "open");
    l.channel.onclose = () => { if (this.links.get(peer) === l) this.set(l, "failed"); };
    l.channel.onmessage = (e) => { try { this.on.packet?.(peer, JSON.parse(e.data)); } catch (_) { /* not one of ours */ } };
    pc.onicecandidate = ({ candidate }) => { if (candidate) this.signal(peer, { rtc: { candidate } }); };
    pc.onconnectionstatechange = () => { if (pc.connectionState === "failed") this.set(l, "failed"); };
    if (!l.offers) return;
    pc.onnegotiationneeded = async () => {
      try {
        await pc.setLocalDescription();
        this.signal(peer, { rtc: { description: pc.localDescription } });
      } catch (e) {
        console.warn("session: negotiating", e);
      }
    };
  }
  /** Something of WebRTC's negotiation, from a peer. */
  async signalled(peer, { description, candidate }) {
    const l = this.links.get(peer);
    if (!l) return;
    const { pc } = l;
    try {
      if (description) {
        await pc.setRemoteDescription(description);
        if (description.type === "offer") {
          await pc.setLocalDescription();
          this.signal(peer, { rtc: { description: pc.localDescription } });
        }
      } else if (candidate) await pc.addIceCandidate(candidate);
    } catch (e) {
      console.warn("session: negotiating", e);
    }
  }
  /** To everyone it can reach, now or not at all. */
  send(value) {
    let text = null;
    for (const l of this.links.values()) {
      if (l.state !== "open" || l.channel.readyState !== "open") continue;
      text ??= JSON.stringify(value);
      l.channel.send(text);
    }
  }
  drop(peer) {
    const l = this.links.get(peer);
    if (!l) return;
    clearTimeout(l.timer);
    this.links.delete(peer);
    l.pc.close();
  }
  close() {
    for (const peer of [...this.links.keys()]) this.drop(peer);
  }
}
