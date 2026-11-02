// A session's files, as this person has them (docs/collaboration.md, "One ordered log"). The room's order is the truth, and merging is
// git's three ways, by the same merge a pull uses: an entry says which of the file's versions it was made from (`over`), and when its turn
// comes, what it changed from that is merged with what the file now is. Where it clashes it is dropped, by everyone alike. It stays in the
// log, and its author can have it back.
//
// What this person sees is the log so far with their own changes over it, so nothing waits for the room. One edit of a file is on its way
// at a time, so that what each is made from is a version everyone has: what is changed meanwhile waits here, and goes next, as one.
//
// Two levels, for a file being typed in: `live` is its text as it is typed; `built` is what the model is built from, which moves on when
// someone says so (a save, or anything but typing), so that a half-typed line breaks nobody's model.
//
// It knows nothing of rooms or of where files are kept: an edit comes out of `edit` or `heard` to be said, and entries go in to `heard`.

/** Three ways, of files that may not be there (null): the text it comes to (null: gone), or undefined where they clash. */
function threeWay(merge, file, base, ours, theirs) {
  if (theirs === base || ours === theirs) return ours;
  if (ours === base) return theirs;
  // (Taken away on one side, and changed on the other.)
  if (ours == null || theirs == null) return undefined;
  return merge(file, base ?? "", ours, theirs) ?? undefined;
}

/** What makes `was` into `now`: the lines between what they share at each end. (Exact, of that text: not to be applied to any other.) */
export function deltaOf(was, now) {
  const a = was.split("\n"), b = now.split("\n");
  let at = 0;
  while (at < a.length && at < b.length && a[at] === b[at]) at++;
  let tail = 0;
  while (tail < a.length - at && tail < b.length - at && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  return { at, del: a.length - at - tail, ins: b.slice(at, b.length - tail) };
}
export function withDelta(was, { at, del, ins }) {
  const lines = was.split("\n");
  lines.splice(at, del, ...ins);
  return lines.join("\n");
}

/** A text's sum, to tell whether two people's are the same. */
function sumOf(text) {
  if (text == null) return 0;
  let h = 0x811c9dc5;
  for (let k = 0; k < text.length; k++) h = Math.imul(h ^ text.charCodeAt(k), 0x01000193);
  return h >>> 0;
}

export class Replica {
  /** `origin(file)`: the text a file has at the commit the session began on (null: none). It may be asked for at any time, and is asked for
   *  once of each: whoever makes this has it read before an entry or an edit of the file comes (`has`, `load`). `merge(file, base, ours,
   *  theirs)`: three ways, the text or nothing where they clash (topo-sync's `merge_file`; everyone's must be the same). `newId()`: for an edit. */
  constructor(origin, merge, newId) {
    Object.assign(this, { origin, merge, newId });
    /** Each file's versions as the log has made them, by which an edit says what it was made from. */
    this.versions = new Map();
    this.builtText = new Map();
    /** This person's changes not yet in the log, by file: `{ sent: the edit on its way ({ id, text, build }), view: all of them over the
     *  log so far, buildQueued: the next is to be built from, builtView: what they build from till then, lost: what could not stay in view }`. */
    this.own = new Map();
    /** The files any edit has touched. */
    this.touched = new Set();
  }
  has(file) {
    return this.versions.has(file);
  }
  /** A file's text as the commit has it, known from here on. */
  load(file, text) {
    if (!this.has(file)) {
      this.versions.set(file, [text]);
      this.builtText.set(file, text);
    }
  }
  async ensure(file) {
    if (!this.has(file)) this.load(file, await this.origin(file));
  }
  /** The file as the log so far has it. */
  confirmed(file) {
    return this.versions.get(file)?.at(-1) ?? null;
  }
  /** A file as this person sees it. */
  live(file) {
    const o = this.own.get(file);
    return o ? o.view : this.confirmed(file);
  }
  /** What the model is built from: as far as the log has been told to build, or this person's own, where they have said to. */
  built(file) {
    const o = this.own.get(file);
    return o && (o.buildQueued || o.sent?.build) ? o.builtView : this.builtText.get(file) ?? null;
  }
  /** Whether nothing of this person's is waiting for the log. */
  get settled() {
    return this.own.size === 0;
  }

  /** Makes the file this text, as this person's edit: the entry to say now, or null (there is nothing to say, or it waits for the last). `build`:
   *  the model is to be built from it (a save; anything but typing). `base`: the text it was made from, where that may not be the file as it now is
   *  (it was read, or built, a moment ago): what was changed from that goes over what has come since, which it does not take back. `{ clash: true }`:
   *  that clashes with what has come. */
  edit(file, asked, { build = true, base } = {}) {
    const now = this.live(file), text = base === undefined || base === now ? asked : threeWay(this.merge, file, base, now, asked);
    if (text === undefined) return { clash: true };
    if (text === now && (!build || this.built(file) === text)) return null;
    let o = this.own.get(file);
    if (!o) this.own.set(file, (o = { sent: null, view: now, buildQueued: false, builtView: null, lost: null }));
    o.view = text;
    if (build) { o.buildQueued = true; o.builtView = text; }
    this.touched.add(file);
    return this.next(file);
  }
  /** The edit to say next of a file, if there is one to say and none is on its way. */
  next(file) {
    const o = this.own.get(file);
    if (!o || o.sent) return null;
    const was = this.confirmed(file);
    if (o.view === was && !o.buildQueued) { this.own.delete(file); return null; }
    const e = { id: this.newId(), file, over: this.versions.get(file).length - 1, build: o.buildQueued };
    if (o.view == null) e.gone = true;
    else if (was == null) e.text = o.view;
    else e.delta = deltaOf(was, o.view);
    o.sent = { id: e.id, text: o.view, build: o.buildQueued };
    o.buildQueued = false;
    return e;
  }
  /** What this person's changes come to, the text they were over having become this. */
  moved(file, o, from, to) {
    const rebased = threeWay(this.merge, file, from, o.view, to);
    if (rebased === undefined) o.lost ??= o.view;
    o.view = rebased === undefined ? to : rebased;
    if (o.buildQueued || o.sent?.build) o.builtView = threeWay(this.merge, file, from, o.builtView, to) ?? to;
  }

  /** An edit of the log's, in its order. What it did: `{ applied, own, dropped, next }`, with what to say next of this person's; and of their own
   *  that is dropped, what it was (`mine`) and what it was made from (`base`), to have it back. (Everyone drops what clashes alike, since each has
   *  the same text at this place.) */
  heard({ id, file, over, gone, text, delta, build }) {
    const vs = this.versions.get(file), base = vs[over], was = vs.at(-1);
    const theirs = gone ? null : text !== undefined ? text : withDelta(base, delta);
    const now = threeWay(this.merge, file, base, was, theirs), applied = now !== undefined;
    if (applied) {
      if (now !== was) vs.push(now);
      if (build) this.builtText.set(file, now);
    }
    const to = applied ? now : was, o = this.own.get(file), own = o?.sent?.id === id;
    this.touched.add(file);
    const said = { applied, own, dropped: !applied };
    if (own) {
      if (!applied) { said.mine = o.lost ?? o.view; said.base = base; }
      this.moved(file, o, o.sent.text, to);
      o.sent = null;
      o.lost = null;
      said.next = this.next(file);
    } else if (o && applied && now !== was) {
      // (What is on its way moves with it, though it may be going to be dropped; and what is in view with that.)
      const rebased = o.sent ? threeWay(this.merge, file, was, o.sent.text, to) : to, sent = rebased === undefined ? to : rebased;
      if (rebased === undefined) o.lost ??= o.view;
      this.moved(file, o, o.sent ? o.sent.text : was, sent);
      if (o.sent) o.sent.text = sent;
    }
    return said;
  }

  /** An edit of this person's that the log will not have (too large to say, say): what the file now is, and what to say next. */
  forget(id, file) {
    const o = this.own.get(file);
    if (o?.sent?.id !== id) return { live: this.live(file), next: null };
    this.moved(file, o, o.sent.text, this.confirmed(file));
    o.sent = null;
    return { next: this.next(file), live: this.live(file) };
  }

  /** What this person has of each file touched, for others to compare: `{ file: [version, its sum] }`. */
  sums() {
    const out = {};
    for (const file of this.touched) { const vs = this.versions.get(file); if (vs) out[file] = [vs.length - 1, sumOf(vs.at(-1))]; }
    return out;
  }
  /** Of what someone else says they have, the files where the same version is not the same text here: the merges were not the same. */
  differs(theirs) {
    return Object.entries(theirs ?? {}).filter(([file, [v, sum]]) => { const vs = this.versions.get(file); return vs && v < vs.length && sumOf(vs[v]) !== sum; }).map(([file]) => file);
  }
}
