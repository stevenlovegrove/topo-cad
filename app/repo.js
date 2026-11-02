// A git repository, as a page reaches it: its objects by their shas, and its branches. What a branch's working copy (work.js)
// is made over, wherever the repository is. Each place it may be is a kind of this:
//
//   GitHub     github.js: GitHub's API, from the page
//   HostRepo   host-git.js: the repository a project is in, on the `topo serve` the page is served by
//
// Each says:
//
//   name                               what it is called to people ("GitHub")
//   pushes                             whether a commit here goes on to where others are (no local commits)
//   upstream(branch)                   what the branch is called where others see it ("origin/main")
//   kindOf(branch)                     the location (project.js) a page is of, on that branch
//   look(branch)                       where a branch is, as cheaply as can be found out
//   head(branch), branches(), branch(name, at), compare(from, to), log(from, count)
//   commit(branch, expected, changes, message)        one commit of these files (`{ path: text }`; null: removed), or Stale
//   makeCommit(parents, tree, changes, message), moveBranch(branch, sha)    the same in steps, for a commit of two parents
//   explain(e), openTrouble(e, branch), unreadable(e)    what went wrong, in words
//   joining                            what someone invited to a session on it needs
//   readCommit(sha), readTree(commit), readBlob(sha)     what never changes, read once (below)

/** A commit said to follow a head that is no longer the branch's: read it again, merge, and try again. */
export class Stale extends Error {}
/** A commit this person may not make, there: trying again will not help. */
export class Refused extends Error {}

export class Repo {
  /** `kept`: `{ get, set }` of what never changes (a blob, a commit, a tree, each by its sha), for when the page is loaded again. */
  constructor({ kept = null } = {}) {
    this.kept = kept;
    /** What has been read, by blob and by commit. */
    this.blobs = new Map();
    this.commits = new Map();
  }
  /** Where something is kept (each kind of repository says, by what makes it another's). */
  keptName(key) {
    return key;
  }
  /** Something made here that never changes, kept as if it had been read. */
  keep(key, value) {
    this.kept?.set(this.keptName(key), value).catch(() => {});
  }
  /** What never changes: as kept, else `read()`, and kept. */
  async once(key, read) {
    const name = this.keptName(key);
    if (!this.kept) return read();
    const had = await this.kept.get(name).catch(() => undefined);
    if (had !== undefined) return had;
    const got = await read();
    this.kept.set(name, got).catch(() => {});
    return got;
  }
  /** A commit's parents (the first is the side it was made on) and its tree. What a commit is never changes. */
  commitOf(sha) {
    const had = this.commits.get(sha);
    if (had) return had;
    const c = this.once(`commit:${sha}`, () => this.readCommit(sha));
    this.commits.set(sha, c);
    c.catch(() => this.commits.delete(sha));
    return c;
  }
  /** What a commit has: path → blob sha (which is of the file's text alone: a file with the same one has not changed).
   *  (`tree`: as the repository says it.) */
  async files(commit) {
    const got = await this.once(`tree:${commit}`, () => this.readTree(commit));
    return { tree: got.tree, files: new Map(got.files) };
  }
  /** A file's text, by its blob. (A blob never changes: it is read once.) */
  blob(sha) {
    const had = this.blobs.get(sha);
    if (had) return had;
    const text = this.once(`blob:${sha}`, () => this.readBlob(sha));
    this.blobs.set(sha, text);
    text.catch(() => this.blobs.delete(sha));
    return text;
  }
  /** A commit made from here that has these files, which are as `changed` says (`{ id, sha, text }`, each with a sha), kept as
   *  if they had been read. */
  remember(commit, files, changed) {
    for (const { sha, text } of changed) {
      if (!sha) continue;
      this.blobs.set(sha, Promise.resolve(text));
      this.keep(`blob:${sha}`, text);
    }
    this.keep(`tree:${commit}`, { tree: commit, files: [...files] });
  }
  /** A file as a commit has it (null: none). */
  async textAt(commit, file) {
    const blob = (await this.files(commit)).files.get(file);
    return blob ? this.blob(blob) : null;
  }
  /** A file as a worker can read its byte ranges, where the repository is somewhere that can: `{ url }`; else null. */
  bytes() {
    return null;
  }
  /** The words for what went wrong, where nothing more is known of it. */
  explain(e) {
    return String(e.message ?? e);
  }
  openTrouble(e) {
    return e;
  }
  unreadable(e) {
    return String(e.message ?? e);
  }
}
