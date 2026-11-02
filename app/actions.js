// What can be done in the app by name, and what has been done in it lately.
//
//   command(name, said, f)   something that can be done by name: `app.name(…)` to a shortcut's
//                            snippet (shortcuts.js). `said`: a line, its call and what it does,
//                            for whoever writes one (a person, or an agent for them).
//   did(name, ...args)       it was done, however (a click, a key, a snippet): kept, the latest
//                            few, as the call that would do it again.
//   recent, onDid(f)         what was done, the latest last; `f()` when something is.
//
// This module knows nothing of what the commands do: whoever can do a thing says so here, and
// says so here when it has been done.

/** What a shortcut's snippet is given as `app`: every command, by its name. */
export const app = {};
/** What each command is: a line each. */
export const says = [];
export function command(name, said, f) {
  app[name] = f;
  says.push(said);
}

/** What was done lately, the latest last: { t (when, ms), run (the call that does it), by (the keys of the shortcut it was done by, if one) }. */
export const recent = [];
const KEPT = 40, heard = [];
let cause = null;
export function did(name, ...args) {
  while (args.length && args.at(-1) === undefined) args.pop();
  const run = `app.${name}(${args.map((a) => JSON.stringify(a ?? null)).join(", ")})`, last = recent.at(-1), t = Date.now();
  // (One thing done is said once, though more than one of those it passes through says it.)
  if (last?.run === run && t - last.t < 50) return;
  recent.push({ t, run, ...(cause ? { by: cause } : {}) });
  if (recent.length > KEPT) recent.shift();
  for (const f of heard) f();
}
export const onDid = (f) => heard.push(f);
/** Does `f()` as what a shortcut did (`by`: its keys): what is done meanwhile is said to be by it. */
export function caused(by, f) {
  const was = cause;
  cause = by;
  try { return f(); } finally { cause = was; }
}
