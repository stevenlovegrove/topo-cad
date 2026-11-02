// Git, as a tool: what a person's click and an agent's call both do to a project's versions, wherever those are kept (work.js says
// what kinds there are). `git(args)` is `workspace.call("git", args)`: `{ text, data, error }`. What a kind cannot do is refused here,
// with why, and is not offered in the Changes panel.
//
//   status                     where HEAD is, what has changed, what is in progress
//   fetch                      looks at where the branch is upstream; changes no files
//   pull                       up to where it is upstream, your changes merged with it (as --autostash)
//   push                       what is committed here, upstream, where a commit does not go there by itself
//   diff (path?)               your changes, as a unified diff
//   commit (message, paths?)   commits your changes, or those files of them (and pushes, where there are no local commits)
//   restore (paths?)           lets your changes go
//   branches                   the repository's branches
//   branch (name)              a new branch from here, your changes going with it, and switches to it
//   switch (name)              to a branch (your changes stay with the one they were made on); or back from a commit checked out
//   merge (name, message?)     merges that branch into this one
//   log (count?)               the latest commits
//   checkout (commit)          looks at a commit, HEAD detached
//   revert (commit)            takes a commit back, as changes to commit
//   conflicts                  what is in conflict, each side's text
//   resolve (path, side | text)  a file in conflict: side "ours" or "theirs", or the text it is to have
//   abort                      leaves a merge, pull or revert in progress undone

import { project, storage } from "./project.js";
import { githubPlace } from "./store.js";
import { Cannot } from "./work.js";

/** What it says of something brought in, however that went. */
const broughtIn = (r, done) => (r?.clashed ? `conflict in ${r.clashed.length} file${r.clashed.length === 1 ? "" : "s"}: ${r.clashed.join(", ")}` : r?.left ? `still unmerged: ${r.left.join(", ")}` : done(r));

const asStatus = (s) => [
  s.viewing ? `HEAD detached at ${s.commit?.slice(0, 7) ?? "?"} (branch ${s.branch})` : `On branch ${s.branch}, at ${s.commit?.slice(0, 7) ?? "?"}`,
  s.moved && `Behind ${s.upstream} (at ${s.head.slice(0, 7)}): pull to update`,
  s.merging && `Merging ${s.merging.branch}: commit to conclude`,
  s.conflicts.length && `${s.doing}: unmerged: ${s.conflicts.join(", ")}`,
  s.edited.length ? `Changes: ${s.edited.map((e) => e.path + (e.gone ? " (deleted)" : "")).join(", ")}` : "No changes",
  s.trouble && `Could not fetch: ${s.trouble}`,
].filter(Boolean).join("\n");

/** Goes to a branch that is another page, as another project is: listed here, where that is how they are known. */
async function goTo(kind) {
  const branch = githubPlace(kind);
  if (branch) await storage.followBranch(branch.repo, branch.branch);
  location.assign(project.href(project.current(), kind));
}

async function act(work, { action = "status", ...a }) {
  if (!work.can(action)) throw new Cannot(work.whyNot(action));
  const branchNow = async () => (await work.status()).branch, at = async () => ((await work.status()).commit ?? "").slice(0, 7);
  const say = async (text, data) => {
    // (The files may be others now: the model is built from them.)
    await project.touched();
    return { text, data };
  };
  switch (action) {
    case "status": {
      const s = await work.status();
      return { text: asStatus(s), data: s };
    }
    case "fetch": {
      await work.fetch();
      const s = await work.status();
      return { text: asStatus(s), data: s };
    }
    case "diff":
      return { text: (await work.diff(a.path)) || "no changes" };
    case "commit": {
      const sha = await work.commit(a.message, a.paths), s = await work.status();
      return say(`committed${s.pushes ? " and pushed" : ""} ${sha.slice(0, 7)} to ${s.branch}`, { sha });
    }
    case "push": {
      await work.push();
      return say(`pushed ${await branchNow()}`);
    }
    case "restore":
      await work.restore(a.paths);
      return say(a.paths ? `restored ${a.paths.join(", ")}` : "restored all changes");
    case "pull": {
      const r = await work.pull();
      const done = r.pulled ? `pulled ${await branchNow()}: now at ${await at()}` : "already up to date";
      return say(broughtIn(r, () => done), r);
    }
    case "branches": {
      const names = await work.branches(), current = await branchNow();
      return { text: names.map((n) => (n === current ? `* ${n}` : `  ${n}`)).join("\n"), data: { branches: names, current } };
    }
    case "branch": {
      if (!/^[\w./-]+$/.test(a.name ?? "")) throw new Cannot("a branch's name is a plain name");
      const r = await work.newBranch(a.name);
      if (r.go) { await goTo(r.go); return { text: `created ${a.name}, switching to it` }; }
      return say(r.text);
    }
    case "switch": {
      const r = await work.switchTo(a.name);
      if (r.go) { await goTo(r.go); return { text: `switching to ${a.name}` }; }
      return r.moved ? say(r.text) : { text: r.text };
    }
    case "merge": {
      const r = await work.merge(a.name, a.message);
      return say(broughtIn(r, (x) => x.text), r);
    }
    case "log": {
      const data = await work.log(a.count ?? 30);
      return { text: data.commits.map((c) => `${c.sha.slice(0, 7)} ${c.message.split("\n")[0]}`).join("\n"), data };
    }
    case "checkout":
      await work.view(a.commit);
      return say(`HEAD is now at ${a.commit.slice(0, 7)}, detached`);
    case "revert": {
      const r = await work.revert(a.commit);
      return say(broughtIn(r, () => `reverted ${a.commit.slice(0, 7)} in your changes: commit to keep it`), r);
    }
    case "conflicts": {
      const c = await work.conflicts(), paths = Object.keys(c?.files ?? {});
      return paths.length ? { text: `${c.doing}: ${paths.join(", ")}`, data: c } : { text: "no conflicts", data: { files: {} } };
    }
    case "resolve": {
      if (a.text === undefined && a.side !== "ours" && a.side !== "theirs") throw new Cannot('say a side, "ours" or "theirs", or the text it is to have');
      const r = await work.resolve(a.path, a);
      return say(broughtIn(r, (x) => x?.text ?? "resolved"), r);
    }
    case "abort":
      await work.abort();
      return say("aborted");
    default:
      throw new Cannot(`there is no git action ${action}`);
  }
}

export async function git(args = {}) {
  const work = storage.work;
  if (!work) return { error: "This project has no versions here: it is not in a git repository, or on a branch of one.", text: "", images: [] };
  try {
    return { images: [], ...(await act(work, args)) };
  } catch (e) {
    // (Something that cannot be done is said as that. What is the matter with where it is kept is said as that said it.)
    return { error: String(e.message ?? e), text: "", images: [] };
  }
}
