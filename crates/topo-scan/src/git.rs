//! The project as a git repository (by the `git` program): saves and issue
//! changes are commits by whoever made them; a proposal is a commit on a
//! ref of its own, made without touching the working tree, and applied by
//! cherry-picking it onto the working branch.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

pub struct Repo {
    pub root: PathBuf,
}

/// What a proposal changes: files, and lines added and removed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffStat {
    pub files: Vec<String>,
    pub added: u32,
    pub removed: u32,
}

fn identity(who: &str) -> [(&'static str, String); 4] {
    let slug: String = who.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let email = format!("{}@topo-cad.invalid", if slug.is_empty() { "someone" } else { &slug });
    [("GIT_AUTHOR_NAME", who.to_string()), ("GIT_AUTHOR_EMAIL", email.clone()), ("GIT_COMMITTER_NAME", who.to_string()), ("GIT_COMMITTER_EMAIL", email)]
}

impl Repo {
    /// The repository `dir` is in.
    pub fn open(dir: &Path) -> Result<Repo> {
        let out = Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"]).output().context("running git (is it installed?)")?;
        if !out.status.success() {
            bail!("{} is not in a git repository: a scan project is one (run `git init` in the project's folder)", dir.display());
        }
        Ok(Repo { root: PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).canonicalize()? })
    }

    fn command(&self, args: &[&str], env: &[(&str, String)]) -> Command {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.root).args(args);
        for (k, v) in env {
            c.env(k, v);
        }
        c
    }

    /// Runs git; its output, or its error text.
    fn run_env(&self, args: &[&str], env: &[(&str, String)], stdin: Option<&[u8]>) -> Result<String> {
        let mut c = self.command(args, env);
        c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().context("running git")?;
        if let Some(bytes) = stdin {
            child.stdin.take().unwrap().write_all(bytes)?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!("git {}: {}{}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim(), String::from_utf8_lossy(&out.stdout).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        self.run_env(args, &[], None)
    }

    fn ok(&self, args: &[&str]) -> bool {
        self.command(args, &[]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    /// A path in the repository, relative to its root, with `/`.
    pub fn rel(&self, p: &Path) -> Result<String> {
        let abs = if p.exists() { p.canonicalize()? } else { p.parent().context("path")?.canonicalize()?.join(p.file_name().context("path")?) };
        let r = abs.strip_prefix(&self.root).with_context(|| format!("{} is outside the repository {}", p.display(), self.root.display()))?;
        Ok(r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
    }

    pub fn head(&self) -> Option<String> {
        self.run(&["rev-parse", "--verify", "-q", "HEAD"]).ok().map(|s| s.trim().to_string())
    }

    pub fn branch(&self) -> String {
        self.run(&["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    /// Commits these paths as they are on disk (added, changed or gone), if
    /// they differ from the last commit. Returns the commit made.
    pub fn commit(&self, paths: &[&str], message: &str, who: &str) -> Result<Option<String>> {
        if paths.is_empty() {
            return Ok(None);
        }
        let mut add = vec!["add", "-A", "--"];
        add.extend(paths);
        self.run(&add)?;
        let mut same = vec!["diff", "--cached", "--quiet", "--"];
        same.extend(paths);
        if self.head().is_some() && self.ok(&same) {
            return Ok(None);
        }
        let mut c = vec!["commit", "-q", "-m", message, "--"];
        c.extend(paths);
        self.run_env(&c, &identity(who), None)?;
        Ok(self.head())
    }

    /// Commits every tracked file changed on disk (edits made outside the app).
    pub fn commit_outside_edits(&self) -> Result<Option<String>> {
        if self.head().is_none() || self.ok(&["diff", "--quiet"]) {
            return Ok(None);
        }
        self.run(&["add", "-u"])?;
        self.run_env(&["commit", "-q", "-m", "Edits made outside the app"], &identity("outside"), None)?;
        Ok(self.head())
    }

    /// A commit on `refs/proposals/<name>` whose parent is HEAD and which
    /// sets `files` (paths relative to the root) to the given contents. The
    /// working tree and index are untouched.
    pub fn propose(&self, name: &str, files: &[(String, String)], message: &str, who: &str) -> Result<String> {
        let head = self.head().context("the repository has no commit yet")?;
        // A scratch index in the repository's own git folder (a worktree's `.git` is a file).
        let index = PathBuf::from(self.run(&["rev-parse", "--absolute-git-dir"])?.trim()).join(format!("proposal-index-{}", std::process::id()));
        let mut env: Vec<(&str, String)> = identity(who).into_iter().collect();
        env.push(("GIT_INDEX_FILE", index.display().to_string()));
        let made = (|| -> Result<String> {
            self.run_env(&["read-tree", &head], &env, None)?;
            for (path, text) in files {
                let blob = self.run_env(&["hash-object", "-w", "--stdin"], &env, Some(text.as_bytes()))?;
                self.run_env(&["update-index", "--add", "--cacheinfo", &format!("100644,{},{path}", blob.trim())], &env, None)?;
            }
            let tree = self.run_env(&["write-tree"], &env, None)?;
            let commit = self.run_env(&["commit-tree", tree.trim(), "-p", &head, "-m", message], &env, None)?;
            Ok(commit.trim().to_string())
        })();
        let _ = std::fs::remove_file(&index);
        let commit = made?;
        self.run(&["update-ref", &format!("refs/proposals/{name}"), &commit])?;
        Ok(commit)
    }

    /// The text diff a commit makes.
    pub fn diff(&self, commit: &str) -> Result<String> {
        self.run(&["diff", &format!("{commit}^"), commit])
    }

    pub fn stat(&self, commit: &str) -> Result<DiffStat> {
        let mut st = DiffStat::default();
        for line in self.run(&["diff", "--numstat", &format!("{commit}^"), commit])?.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if let [a, r, path] = f.as_slice() {
                st.added += a.parse().unwrap_or(0);
                st.removed += r.parse().unwrap_or(0);
                st.files.push(path.to_string());
            }
        }
        Ok(st)
    }

    /// A file's text at a revision (None: not there).
    pub fn file_at(&self, rev: &str, path: &str) -> Option<String> {
        self.run(&["show", &format!("{rev}:{path}")]).ok()
    }

    /// Applies a proposal's commit onto the working branch. Edits made
    /// outside the app are committed first; a proposal that no longer
    /// applies (the files moved on) changes nothing and says so.
    pub fn apply(&self, commit: &str, who: &str) -> Result<String> {
        self.commit_outside_edits()?;
        if let Err(e) = self.run_env(&["cherry-pick", "--allow-empty", commit], &identity(who), None) {
            let _ = self.run(&["cherry-pick", "--abort"]);
            bail!("the proposal no longer applies to the files as they are now (nothing changed): {e}");
        }
        self.head().context("no HEAD")
    }

    /// Undoes a commit with a new one.
    pub fn revert(&self, commit: &str, who: &str) -> Result<String> {
        self.commit_outside_edits()?;
        if let Err(e) = self.run_env(&["revert", "--no-edit", commit], &identity(who), None) {
            let _ = self.run(&["revert", "--abort"]);
            bail!("cannot revert (later changes build on it; nothing changed): {e}");
        }
        self.head().context("no HEAD")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh repository in a temporary folder, with `files` committed.
    pub(crate) fn temp_repo(tag: &str, files: &[(&str, &str)]) -> Repo {
        let dir = std::env::temp_dir().join(format!("topo_scan_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(Command::new("git").arg("-C").arg(&dir).args(["init", "-q", "-b", "main"]).status().unwrap().success());
        let repo = Repo::open(&dir).unwrap();
        let mut paths = Vec::new();
        for (p, text) in files {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, text).unwrap();
            paths.push(*p);
        }
        repo.commit(&paths, "start", "tester").unwrap().unwrap();
        repo
    }

    #[test]
    fn a_proposal_is_a_commit_applied_later() {
        let repo = temp_repo("proposal", &[("model.ts", "a\nb\nc\n"), ("other.ts", "x\n")]);
        let head = repo.head().unwrap();
        let p = repo.propose("1-1", &[("model.ts".into(), "a\nB\nc\n".into())], "widen b", "claude").unwrap();
        // Nothing changed yet.
        assert_eq!(repo.head().unwrap(), head);
        assert_eq!(std::fs::read_to_string(repo.root.join("model.ts")).unwrap(), "a\nb\nc\n");
        assert_eq!(repo.stat(&p).unwrap(), DiffStat { files: vec!["model.ts".into()], added: 1, removed: 1 });
        assert!(repo.diff(&p).unwrap().contains("-b\n+B\n"));
        assert_eq!(repo.file_at(&p, "model.ts").unwrap(), "a\nB\nc\n");
        // An edit elsewhere, made outside the app, does not stop it.
        std::fs::write(repo.root.join("other.ts"), "y\n").unwrap();
        let applied = repo.apply(&p, "steven").unwrap();
        assert_eq!(std::fs::read_to_string(repo.root.join("model.ts")).unwrap(), "a\nB\nc\n");
        assert_eq!(std::fs::read_to_string(repo.root.join("other.ts")).unwrap(), "y\n");
        // Proposed by one, applied by another: author and committer.
        let log = repo.run(&["log", "--format=%an/%cn: %s"]).unwrap();
        assert!(log.starts_with("claude/steven: widen b\noutside/outside: Edits made outside the app\n"), "{log}");
        repo.revert(&applied, "steven").unwrap();
        assert_eq!(std::fs::read_to_string(repo.root.join("model.ts")).unwrap(), "a\nb\nc\n");
        std::fs::remove_dir_all(&repo.root).unwrap();
    }

    #[test]
    fn a_stale_proposal_changes_nothing() {
        let repo = temp_repo("stale", &[("model.ts", "a\nb\nc\n")]);
        let p = repo.propose("1-1", &[("model.ts".into(), "a\nB\nc\n".into())], "widen b", "claude").unwrap();
        std::fs::write(repo.root.join("model.ts"), "a\nbee\nc\n").unwrap();
        repo.commit(&["model.ts"], "save", "steven").unwrap().unwrap();
        let e = repo.apply(&p, "steven").unwrap_err().to_string();
        assert!(e.contains("no longer applies"), "{e}");
        assert_eq!(std::fs::read_to_string(repo.root.join("model.ts")).unwrap(), "a\nbee\nc\n");
        assert!(repo.ok(&["diff", "--quiet"]) && repo.ok(&["diff", "--cached", "--quiet"]));
        std::fs::remove_dir_all(&repo.root).unwrap();
    }
}
