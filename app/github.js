// A repository on GitHub, as a page can reach it with nothing between: REST and GraphQL at api.github.com are the
// only parts of GitHub that answer another site's page (not git's own protocol, LFS, tarballs or the sign-in
// endpoints). One kind of repository (repo.js); web/tools/github-sync.mjs measures it.

import { Refused, Repo, Stale } from "./repo.js";
export { Refused, Stale };

const API = "https://api.github.com";
/** How many files are read at once: GitHub turns away a client with too many requests going. */
const READING_AT_ONCE = 8;

const base64 = (text) => {
  const bytes = new TextEncoder().encode(text);
  let out = "";
  for (let k = 0; k < bytes.length; k += 0x8000) out += String.fromCharCode(...bytes.subarray(k, k + 0x8000));
  return btoa(out);
};

export class GitHub extends Repo {
  /** `repo`: "owner/name"; `token`: none, what anyone may read of a public one; or a function giving one (`again`: the last
   *  was turned down), for one that runs out; `api`: where GitHub's API is; `kept`: `{ get, set }` of what never changes
   *  (a blob, a commit, a tree, each by its sha), for when the page is loaded again. */
  constructor({ token = null, repo, api = API, kept = null }) {
    super({ kept });
    this.token = token;
    this.repo = repo;
    this.api = api;
    /** What has been polled: path → { etag, value }. */
    this.polled = new Map();
    /** Files being read, and those waiting to be. */
    this.reading = 0;
    this.waiting = [];
  }
  get name() { return "GitHub"; }
  get pushes() { return true; }
  upstream(branch) { return `origin/${branch}`; }
  kindOf(branch) { return `github:${this.repo}@${branch}`; }
  /** What someone invited to a session on it needs. */
  get joining() { return `You need to be able to read ${this.repo} on GitHub.`; }
  /** Why something done to the branch did not get there, in words. */
  explain(e) {
    if (e instanceof Refused) return /by integration/.test(e.message) ? "You may not commit here: either you have no write access to this repository, or the App is not installed on it" : e.message;
    if (e.status === 401) return "GitHub refused the token: it may have expired";
    if (e.status === 403 || e.status === 429) return `GitHub turned this away for now${e.retryAfter ? `: try again in ${e.retryAfter} s` : ""}`;
    return String(e.message ?? e);
  }
  /** What is said where a branch cannot be opened. */
  openTrouble(e, branch) {
    if (e.status === 401) return new Error("GitHub refused the token: it may have expired");
    return e.status === 404 ? new Error(`${this.repo} has no branch ${branch} that can be seen from here${typeof this.token === "function" ? " (a private repository needs the App installed on it)" : this.token ? "" : " (a private repository needs a token)"}`) : e;
  }
  /** Why this person cannot read it, for those who asked them in. */
  unreadable(e) {
    return e.status === 404 || e.status === 403 ? `${this.repo} is not one this person can read${this.token ? ", or the App is not installed on it" : ": they are not signed in"}` : String(e.message ?? e);
  }

  async send(path, { method = "GET", body, headers = {}, cache = "no-store", accept = "application/vnd.github+json" } = {}) {
    const given = typeof this.token === "function";
    const go = (token) => fetch(`${this.api}/${path}`, {
      method,
      cache,
      headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), Accept: accept, ...(body ? { "Content-Type": "application/json" } : {}), ...headers },
      body: body ? JSON.stringify(body) : undefined,
    });
    const token = given ? await this.token() : this.token, r = await go(token);
    if (r.status !== 401 || !given) return r;
    // (Turned down, with one that can be refreshed: once more with a new one.)
    const again = await this.token(true);
    return again && again !== token ? go(again) : r;
  }
  async json(path, options) {
    const r = await this.send(`repos/${this.repo}/${path}`, options);
    if (!r.ok) {
      const text = (await r.text()).slice(0, 300), retryAfter = Number(r.headers.get("Retry-After")) || null;
      // (A 403 is either being turned away for now, which says so, or a token that may not.)
      if (r.status === 403 && !retryAfter && !/rate limit|abuse/i.test(text)) throw new Refused(/"message":\s*"([^"]+)"/.exec(text)?.[1] ?? text);
      const e = new Error(`${options?.method ?? "GET"} ${path}: ${r.status} ${text}`);
      e.status = r.status;
      e.retryAfter = retryAfter;
      throw e;
    }
    return r.json();
  }

  readCommit(sha) {
    return this.json(`git/commits/${sha}`).then((j) => ({ tree: j.tree.sha, parents: j.parents.map((p) => p.sha) }));
  }
  /** A commit, on no branch yet: these files (`{ path: text }`; null: removed) over a tree, from these parents. Its
   *  `{ sha, tree }`. Three requests with moving a branch to it; `commit` is one, where it can be used. */
  async makeCommit(parents, baseTree, changes, message) {
    const tree = Object.entries(changes).map(([path, text]) => (text == null ? { path, mode: "100644", type: "blob", sha: null } : { path, mode: "100644", type: "blob", content: text }));
    // (No changes, as a merge that keeps all of theirs: the tree it is over. GitHub will not make a tree of none.)
    const sha = tree.length ? (await this.json("git/trees", { method: "POST", body: { base_tree: baseTree, tree } })).sha : baseTree;
    const c = await this.json("git/commits", { method: "POST", body: { message, tree: sha, parents } });
    this.commits.set(c.sha, Promise.resolve({ tree: sha, parents }));
    return { sha: c.sha, tree: sha };
  }
  /** Moves a branch to a commit that comes from where it is; else Stale. */
  async moveBranch(branch, sha) {
    try {
      await this.json(`git/refs/heads/${branch}`, { method: "PATCH", body: { sha, force: false } });
    } catch (e) {
      if (e.status === 422 || e.status === 409) throw new Stale(e.message);
      throw e;
    }
  }

  /** Whether what is at `path` has changed since it was last polled: `{ changed, value }`. Answered "no" by an ETag,
   *  which costs nothing of the hourly limit. (An `If-None-Match` of the page's own keeps the browser's cache out of
   *  it; without one the browser answers from what it has for a minute: see the tool.) */
  async poll(path) {
    const seen = this.polled.get(path);
    const r = await this.send(`repos/${this.repo}/${path}`, { headers: seen ? { "If-None-Match": seen.etag } : {} });
    if (r.status === 304) return { changed: false, value: seen.value };
    if (!r.ok) {
      const e = new Error(`poll ${path}: ${r.status}`);
      e.status = r.status;
      e.retryAfter = Number(r.headers.get("Retry-After")) || null;
      throw e;
    }
    const value = await r.json();
    this.polled.set(path, { etag: r.headers.get("ETag"), value });
    return { changed: true, value };
  }

  /** The commit a branch is at. */
  async head(branch) {
    return (await this.json(`git/ref/heads/${branch}`)).object.sha;
  }
  /** The same, as cheaply as can be: asked by an ETag, which costs nothing where it has not moved. */
  async look(branch) {
    return (await this.poll(`git/ref/heads/${branch}`)).value.object.sha;
  }
  /** (`tree`: as GitHub says it, which asked by a commit is that commit: for a commit's own tree, `commitOf`.) */
  async readTree(commit) {
    const t = await this.json(`git/trees/${commit}?recursive=1`);
    if (t.truncated) throw new Error(`${this.repo} has too many files to be listed at once`);
    return { tree: t.sha, files: t.tree.filter((e) => e.type === "blob").map((e) => [e.path, e.sha]) };
  }
  /** Where something is kept. (What was read with a token is not there for what has none: it may be of a private repository.) */
  keptName(key) {
    return `${this.token ? "token" : "none"}\n${this.repo}\n${key}`;
  }
  /** (And the browser may keep it too.) */
  readBlob(sha) {
    return this.oneAtATime(async () => {
      const r = await this.send(`repos/${this.repo}/git/blobs/${sha}`, { accept: "application/vnd.github.raw+json", cache: "default" });
      if (!r.ok) throw new Error(`blob ${sha}: ${r.status}`);
      return r.text();
    });
  }
  /** `f()`, once fewer than READING_AT_ONCE others are going. */
  async oneAtATime(f) {
    if (this.reading >= READING_AT_ONCE) await new Promise((go) => this.waiting.push(go));
    this.reading++;
    try {
      return await f();
    } finally {
      this.reading--;
      this.waiting.shift()?.();
    }
  }

  /** One commit of these files (`{ path: text, … }`; null: removed) on a branch that is to be at `expected`: one
   *  request, all or nothing. Its sha; or throws Stale. */
  async commit(branch, expected, changes, message) {
    const paths = Object.keys(changes);
    const fileChanges = {
      additions: paths.filter((p) => changes[p] != null).map((path) => ({ path, contents: base64(changes[path]) })),
      deletions: paths.filter((p) => changes[p] == null).map((path) => ({ path })),
    };
    const query = "mutation($input: CreateCommitOnBranchInput!) { createCommitOnBranch(input: $input) { commit { oid } } }";
    const input = { branch: { repositoryNameWithOwner: this.repo, branchName: branch }, message: { headline: message }, expectedHeadOid: expected, fileChanges };
    const r = await this.send("graphql", { method: "POST", body: { query, variables: { input } } });
    if (!r.ok) {
      const e = new Error(`commit: ${r.status}`);
      e.status = r.status;
      e.retryAfter = Number(r.headers.get("Retry-After")) || null;
      throw e;
    }
    const j = await r.json();
    if (j.errors?.length) {
      // (Lost by a moment to another commit, it is said as FORBIDDEN, "is at … but expected …": only the text tells.)
      if (j.errors.some((e) => e.type === "STALE_DATA" || /but expected/.test(e.message))) throw new Stale(j.errors[0].message);
      if (j.errors.some((e) => e.type === "FORBIDDEN")) throw new Refused(j.errors[0].message);
      throw new Error(`commit: ${j.errors[0].type}: ${j.errors[0].message}`);
    }
    return j.data.createCommitOnBranch.commit.oid;
  }

  /** The same by REST, for comparison: a request for each file, its tree, its commit, and moving the branch (the last
   *  is refused where the branch has moved on: Stale; what was made before it stays, unreferenced). */
  async commitByParts(branch, expected, changes, message) {
    const { tree: base } = await this.files(expected);
    const tree = [];
    for (const [path, text] of Object.entries(changes)) {
      if (text == null) { tree.push({ path, mode: "100644", type: "blob", sha: null }); continue; }
      const blob = await this.json("git/blobs", { method: "POST", body: { content: base64(text), encoding: "base64" } });
      tree.push({ path, mode: "100644", type: "blob", sha: blob.sha });
    }
    const t = await this.json("git/trees", { method: "POST", body: { base_tree: base, tree } });
    const c = await this.json("git/commits", { method: "POST", body: { message, tree: t.sha, parents: [expected] } });
    try {
      await this.json(`git/refs/heads/${branch}`, { method: "PATCH", body: { sha: c.sha, force: false } });
    } catch (e) {
      if (e.status === 422 || e.status === 409) throw new Stale(e.message);
      throw e;
    }
    return c.sha;
  }

  /** The repositories the App may reach for the person signed in: `{ full_name, default_branch, private }`, and how many
   *  were more than could be listed at once. */
  async reachable() {
    const of = async (path) => {
      const r = await this.send(path);
      if (!r.ok) { const e = new Error(`${path}: ${r.status}`); e.status = r.status; throw e; }
      return r.json();
    };
    const { installations = [] } = await of("user/installations?per_page=100"), out = new Map();
    let more = 0;
    for (const i of installations) {
      const j = await of(`user/installations/${i.id}/repositories?per_page=100`);
      for (const r of j.repositories ?? []) out.set(r.full_name, { full_name: r.full_name, default_branch: r.default_branch, private: !!r.private });
      more += Math.max(0, (j.total_count ?? 0) - (j.repositories?.length ?? 0));
    }
    return { repositories: [...out.values()].sort((a, b) => a.full_name.localeCompare(b.full_name)), more };
  }
  /** How two commits stand: `{ base, ahead (how many `from` has that `to` has not), behind (how many `to` has that it has not) }`. */
  async compare(from, to) {
    const c = await this.json(`compare/${from}...${to}`);
    return { base: c.merge_base_commit.sha, ahead: c.behind_by, behind: c.ahead_by };
  }
  /** The latest commits of a branch or from a commit: `{ sha, message, author, date, parents }`. */
  async log(from, count = 30) {
    const list = await this.json(`commits?sha=${encodeURIComponent(from)}&per_page=${count}`);
    return list.map((c) => ({ sha: c.sha, message: c.commit.message, author: c.commit.author?.name ?? "", date: c.commit.author?.date ?? "", parents: c.parents.map((p) => p.sha) }));
  }
  /** The repository's branches, by name. */
  async branches() {
    return (await this.json("branches?per_page=100")).map((b) => b.name);
  }

  /** A new branch at a commit. */
  async branch(name, at) {
    await this.json("git/refs", { method: "POST", body: { ref: `refs/heads/${name}`, sha: at } });
  }

  /** How much of each limit is used. (Asking does not count.) */
  async limits() {
    const r = await (await this.send("rate_limit")).json();
    return { core: r.resources.core.used, graphql: r.resources.graphql.used };
  }
}
