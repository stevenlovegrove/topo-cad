// Signing in with GitHub, from the page: the App's code flow, with its secret kept by the relay (relay/src/relay.js), which
// exchanges the code and refreshes the token and keeps neither. What comes back stays in this browser: for this tab alone
// on an address shared with others (store.js).
//
// A token lasts hours, and is refreshed as it is about to run out or if GitHub turns it down. A refresh token works once,
// so one page at a time does it, and another finds what it got.

import { GITHUB_APP } from "./config.js";
import { forgetGitObjects, settings, sharedOrigin } from "./store.js";

const KEPT = "githubAuth", TAB = "topo.github.auth";
/** What a page going to GitHub leaves itself for when it comes back: for this tab alone, whatever the address. */
const ASKED = "topo.github.asked";
/** How long before a token runs out it is refreshed (ms): a request should not be sent with one about to. */
const SOONER = 60000;

const url64 = (bytes) => btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
const random = () => url64(crypto.getRandomValues(new Uint8Array(32)));
const challengeOf = async (verifier) => url64(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));

export async function readKept(shared = sharedOrigin()) {
  if (!shared) return (await settings.get(KEPT).catch(() => null)) ?? null;
  try { return JSON.parse(sessionStorage.getItem(TAB) ?? "null"); } catch (_) { return null; }
}
export async function keep(a, shared = sharedOrigin()) {
  if (!shared) return settings.set(KEPT, a);
  try {
    if (a) sessionStorage.setItem(TAB, JSON.stringify(a));
    else sessionStorage.removeItem(TAB);
  } catch (_) { /* a browser that keeps nothing: signed in again */ }
}

/** A token as GitHub says it, as it is kept. */
const asKept = (t, login) => ({
  access: t.access_token,
  expires: t.expires_in ? Date.now() + t.expires_in * 1000 : null,
  refresh: t.refresh_token ?? null,
  refreshExpires: t.refresh_token_expires_in ? Date.now() + t.refresh_token_expires_in * 1000 : null,
  login,
});

export const auth = {
  /** This copy's App (`clientId`, `slug`, `broker`, `login`, `api`). */
  app: GITHUB_APP,
  /** What is kept of the person signed in: `{ access, expires, refresh, refreshExpires, login }`. */
  kept: null,
  /** Why signing in did not work, where it just did not. */
  trouble: null,
  listeners: [],
  onChange(f) { this.listeners.push(f); },
  said() { for (const f of this.listeners) f(); },

  /** Whether this copy of the app has an App to sign in with. */
  can() { return !!this.app.clientId; },
  signedIn() { return !!this.kept; },
  login() { return this.kept?.login ?? null; },

  /** As the page is loaded: who is signed in; and if GitHub has just sent the page back, that is finished. */
  async boot() {
    this.app = { ...GITHUB_APP, ...((await settings.get("githubApp").catch(() => null)) ?? {}) };
    this.kept = await readKept();
    await this.finish();
  },

  /** Goes to GitHub to sign in, and comes back to this address. */
  async signIn() {
    const state = random(), verifier = random(), back = location.origin + location.pathname;
    sessionStorage.setItem(ASKED, JSON.stringify({ state, verifier, back, then: location.search + location.hash }));
    const q = new URLSearchParams({ client_id: this.app.clientId, state, redirect_uri: back, scope: "offline_access", code_challenge: await challengeOf(verifier), code_challenge_method: "S256" });
    location.assign(`${this.app.login}/login/oauth/authorize?${q}`);
  },

  /** What GitHub said in the page's address, if it has just sent the page back. The address is as it was before going. */
  async finish() {
    const q = new URLSearchParams(location.search), code = q.get("code"), error = q.get("error"), state = q.get("state");
    if (!state || (!code && !error)) return;
    let asked = null;
    try { asked = JSON.parse(sessionStorage.getItem(ASKED) ?? "null"); sessionStorage.removeItem(ASKED); } catch (_) { /* as below */ }
    // (The code is out of the address whatever happens: it is not to be left in a history or a link.)
    history.replaceState(history.state, "", (asked?.back ?? location.pathname) + (asked?.then ?? ""));
    // (Not what this tab asked for: someone may be trying to sign this page in as them.)
    if (!asked || asked.state !== state) { this.trouble = "Signing in did not work: GitHub's answer was not to what this tab asked. Try again."; return; }
    if (error) { this.trouble = error === "access_denied" ? "Signing in was turned down." : `Signing in did not work: ${q.get("error_description") ?? error}`; return; }
    try {
      const t = await this.ask("token", { code, verifier: asked.verifier, redirect_uri: asked.back });
      const me = await fetch(`${this.app.api}/user`, { headers: { Authorization: `Bearer ${t.access_token}`, Accept: "application/vnd.github+json" } });
      if (!me.ok) throw new Error(`GitHub would not say who this is: ${me.status}`);
      this.kept = asKept(t, (await me.json()).login);
      await keep(this.kept);
    } catch (e) {
      this.trouble = `Signing in did not work: ${e.message ?? e}`;
    }
  },
  /** Asks the relay: what it passes back from GitHub. */
  async ask(route, body) {
    const r = await fetch(`${this.app.broker}/auth/github/${route}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) {
      const e = new Error(j.error_description ?? j.error ?? `the relay said ${r.status}`);
      e.said = j.error;
      throw e;
    }
    return j;
  },

  /** A token to use now (none: not signed in). `again`: GitHub has just turned the last one down. Bound: passed as it is. */
  token: async function token(again = false) {
    const a = auth;
    if (!a.kept) return null;
    if (!again && (!a.kept.expires || a.kept.expires - SOONER > Date.now())) return a.kept.access;
    if (!a.kept.refresh) { if (again) await a.dropped(); return a.kept?.access ?? null; }
    return (a.refreshing ??= a.refreshed(a.kept.access).finally(() => { a.refreshing = null; }));
  },
  /** A new token. (Another page may have got it first: then it is that one, and the refresh token, which works once, is theirs.) */
  async refreshed(was) {
    const go = async () => {
      const now = await readKept();
      if (now?.access && now.access !== was) { this.kept = now; return now.access; }
      try {
        const t = await this.ask("refresh", { refresh_token: this.kept.refresh });
        this.kept = asKept(t, this.kept.login);
        await keep(this.kept);
        return this.kept.access;
      } catch (e) {
        // (GitHub said it is no good. Not reaching the relay is not that: it is tried again.)
        if (e.said) { await this.dropped(); return null; }
        throw e;
      }
    };
    return navigator.locks ? navigator.locks.request("topo-github-refresh", go) : go();
  },
  /** Signed in no longer, by GitHub's say. */
  async dropped() {
    this.kept = null;
    await keep(null);
    this.trouble = "GitHub no longer takes this sign-in: sign in again.";
    this.said();
  },
  /** Signs out: GitHub is told to take the token back, as far as it can be told, and nothing is kept here. */
  async signOut() {
    const access = this.kept?.access;
    this.kept = null;
    await keep(null);
    await settings.del("githubReachable").catch(() => {});
    await forgetGitObjects();
    if (access) await this.ask("revoke", { access_token: access }).catch(() => {});
    this.said();
  },
};
