//! Issues: questions and faults pinned to places, each with a thread, kept
//! as one JSON file per issue in the project's repository (`issues/N.json`)
//! so their history is the repository's and they merge cleanly.
//!
//! Everyone who acts is a [`Participant`]: a person at a browser or an
//! agent over MCP.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::V3;

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    /// Stable: what entries and assignments refer to.
    pub id: String,
    pub name: String,
    /// "person" or "agent".
    pub kind: String,
    /// A CSS colour for their badge.
    pub colour: String,
}

const COLOURS: [&str; 8] = ["#378ADD", "#D85A30", "#1D9E75", "#D4537E", "#7F77DD", "#BA7517", "#639922", "#E24B4A"];
const WORDS: [&str; 24] = ["Alder", "Birch", "Cedar", "Dowel", "Eave", "Fir", "Gable", "Hazel", "Iron", "Joist", "Kerf", "Larch", "Maple", "Newel", "Oak", "Purlin", "Quoin", "Rafter", "Sill", "Tenon", "Umber", "Valley", "Wedge", "Yew"];

/// Where an issue is: a box in the scan's frame (a point is a small box),
/// and what it was pinned to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub min: V3,
    pub max: V3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    /// A fitted plane's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<String>,
    /// A slab's or a polyline's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

impl Place {
    pub fn about(p: V3, r: f32) -> Place {
        Place { min: [p[0] - r, p[1] - r, p[2] - r], max: [p[0] + r, p[1] + r, p[2] + r], member: None, selection: None, plane: None, object: None }
    }
    pub fn centre(&self) -> V3 {
        std::array::from_fn(|k| 0.5 * (self.min[k] + self.max[k]))
    }
}

/// What a proposal did to the evidence: members supported, contradicted, unseen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub supported: u32,
    pub contradicted: u32,
    pub unseen: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    /// Something said; `view` is a partial view state to follow as a link.
    Comment {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view_label: Option<String>,
    },
    /// A tool an agent ran while working the issue.
    Tool { name: String, summary: String },
    /// What the scan shows, with what is needed to weigh it.
    Evidence { title: String, text: String },
    /// A change to the model's files, as a commit on `refs/proposals/<issue>-<n>`.
    Proposal {
        n: u32,
        title: String,
        commit: String,
        files: Vec<String>,
        added: u32,
        removed: u32,
        #[serde(default)]
        members_added: Vec<String>,
        #[serde(default)]
        members_removed: Vec<String>,
        before: Tally,
        after: Tally,
        /// "open", "applied" (with the commit it became), "rejected" or "reverted".
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        applied_as: Option<String>,
    },
    /// A change of status, priority, assignee or title.
    Change { what: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// A participant's id.
    pub who: String,
    pub time: f64,
    #[serde(flatten)]
    pub body: Body,
}

pub const STATUSES: [&str; 4] = ["waiting", "doing", "open", "done"];
pub const CATEGORIES: [&str; 3] = ["scan", "note", "check"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub id: u32,
    pub title: String,
    /// "scan", "note" or "check" (raised by the model's own checks).
    pub category: String,
    /// "open", "doing" (in hand), "waiting" (on `waiting_on`) or "done".
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<String>,
    /// 1 high, 2 normal, 3 low.
    pub priority: u8,
    /// Order within a column of the board (smaller first).
    #[serde(default)]
    pub rank: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub by: String,
    pub time: f64,
    /// None: pinned nowhere (the general thread, a note).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// Of a check's issue: what identifies the check (its kind and members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

pub fn priority_name(p: u8) -> &'static str {
    match p {
        1 => "high",
        3 => "low",
        _ => "normal",
    }
}
pub fn priority_from(name: &str) -> u8 {
    match name {
        "high" => 1,
        "low" => 3,
        _ => 2,
    }
}

/// The issues and participants of a project, in `dir` (`<repo>/issues`).
pub struct Store {
    pub dir: PathBuf,
    pub issues: Vec<Issue>,
    pub participants: Vec<Participant>,
}

impl Store {
    pub fn load(dir: &Path) -> Result<Store> {
        let mut issues = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.strip_suffix(".json").is_some_and(|s| s.parse::<u32>().is_ok()) {
                    let text = std::fs::read_to_string(e.path())?;
                    issues.push(serde_json::from_str::<Issue>(&text).with_context(|| format!("reading {}", e.path().display()))?);
                }
            }
        }
        issues.sort_by_key(|i| i.id);
        let participants = std::fs::read_to_string(dir.join("participants.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Ok(Store { dir: dir.to_path_buf(), issues, participants })
    }

    fn write(&self, name: &str, v: &impl Serialize) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(name);
        std::fs::write(&path, serde_json::to_string_pretty(v)? + "\n")?;
        Ok(path)
    }

    /// Writes an issue's file; the path, to commit.
    pub fn save(&self, id: u32) -> Result<PathBuf> {
        self.write(&format!("{id}.json"), self.get(id)?)
    }

    pub fn get(&self, id: u32) -> Result<&Issue> {
        self.issues.iter().find(|i| i.id == id).with_context(|| format!("no issue #{id}"))
    }
    pub fn get_mut(&mut self, id: u32) -> Result<&mut Issue> {
        self.issues.iter_mut().find(|i| i.id == id).with_context(|| format!("no issue #{id}"))
    }

    /// The participant with this id, made on first sight: a person gets a
    /// random word for a name until they choose one.
    pub fn participant(&mut self, id: &str, kind: &str) -> Result<(Participant, Option<PathBuf>)> {
        if let Some(p) = self.participants.iter().find(|p| p.id == id) {
            return Ok((p.clone(), None));
        }
        let n = self.participants.len();
        let name = if kind == "agent" {
            id.to_string()
        } else {
            // A word nobody here has yet.
            let seed = id.bytes().fold(n, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
            (0..WORDS.len()).map(|k| WORDS[(seed + k) % WORDS.len()]).find(|w| !self.participants.iter().any(|p| p.name == *w)).unwrap_or("Guest").to_string()
        };
        let p = Participant { id: id.to_string(), name, kind: kind.to_string(), colour: COLOURS[n % COLOURS.len()].to_string() };
        self.participants.push(p.clone());
        Ok((p, Some(self.write("participants.json", &self.participants)?)))
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<PathBuf> {
        let name = name.trim();
        if name.is_empty() {
            bail!("a name cannot be empty");
        }
        self.participants.iter_mut().find(|p| p.id == id).with_context(|| format!("no participant {id}"))?.name = name.to_string();
        self.write("participants.json", &self.participants)
    }

    pub fn name_of(&self, id: &str) -> String {
        self.participants.iter().find(|p| p.id == id).map_or_else(|| id.to_string(), |p| p.name.clone())
    }

    /// A new issue, raised by `who`; its id.
    pub fn create(&mut self, who: &str, title: &str, category: &str, place: Option<Place>, priority: u8) -> u32 {
        let id = self.issues.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let t = now();
        self.issues.push(Issue { id, title: title.to_string(), category: category.to_string(), status: "open".into(), waiting_on: None, priority: priority.clamp(1, 3), rank: t, assignee: None, by: who.to_string(), time: t, place, key: None, entries: Vec::new() });
        id
    }

    pub fn push(&mut self, id: u32, who: &str, body: Body) -> Result<()> {
        self.get_mut(id)?.entries.push(Entry { who: who.to_string(), time: now(), body });
        Ok(())
    }

    /// Adds to an issue's thread. A question from someone else to the
    /// issue's raiser leaves the card waiting on them; an answer from the
    /// one waited on hands it back.
    ///
    /// An agent asking in an issue it raised itself waits on no one in
    /// particular (`waiting_on` is None): any person's reply hands it back.
    pub fn comment(&mut self, id: u32, who: &str, text: &str, view: Option<Value>, view_label: Option<String>) -> Result<()> {
        let agent = |id: &str| self.participants.iter().any(|p| p.id == id && p.kind == "agent");
        let (asker_is_agent, i) = (agent(who), self.get(id)?);
        let answered = i.status == "waiting" && (i.waiting_on.as_deref() == Some(who) || (i.waiting_on.is_none() && !asker_is_agent));
        let asks = text.trim_end().ends_with('?') && i.status != "done" && (i.by != who || asker_is_agent);
        let i = self.get_mut(id)?;
        if answered {
            i.status = if i.assignee.is_some() { "doing".into() } else { "open".into() };
            i.waiting_on = None;
        } else if asks {
            i.status = "waiting".into();
            i.waiting_on = (i.by != who).then(|| i.by.clone());
        }
        self.push(id, who, Body::Comment { text: text.to_string(), view, view_label })
    }

    /// Sets what is given, noting it in the thread.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, id: u32, who: &str, status: Option<&str>, priority: Option<u8>, title: Option<&str>, assignee: Option<Option<String>>, rank: Option<f64>) -> Result<()> {
        let names: Vec<(String, String)> = self.participants.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        let i = self.get_mut(id)?;
        let mut what = Vec::new();
        if let Some(s) = status.filter(|s| *s != i.status) {
            if !STATUSES.contains(&s) {
                bail!("status: one of {}", STATUSES.join(", "));
            }
            i.status = s.into();
            if s != "waiting" {
                i.waiting_on = None;
            } else if i.waiting_on.is_none() && i.by != who {
                i.waiting_on = Some(i.by.clone());
            }
            what.push(format!("status: {s}"));
        }
        if let Some(p) = priority.filter(|p| *p != i.priority) {
            i.priority = p.clamp(1, 3);
            what.push(format!("priority: {}", priority_name(i.priority)));
        }
        if let Some(t) = title.filter(|t| *t != i.title) {
            i.title = t.into();
            what.push(format!("title: {t}"));
        }
        if let Some(a) = assignee.filter(|a| *a != i.assignee) {
            what.push(match &a {
                Some(a) => format!("assigned to {}", names.iter().find(|n| &n.0 == a).map_or(a.as_str(), |n| n.1.as_str())),
                None => "unassigned".into(),
            });
            i.assignee = a;
        }
        if let Some(r) = rank {
            i.rank = r;
        }
        if !what.is_empty() {
            i.entries.push(Entry { who: who.to_string(), time: now(), body: Body::Change { what: what.join(", ") } });
        }
        Ok(())
    }

    /// The order to work in: what is in hand, then open by priority and
    /// board order; then what waits on someone; done last.
    pub fn working_order(&self) -> Vec<&Issue> {
        let mut v: Vec<&Issue> = self.issues.iter().collect();
        let col = |s: &str| match s {
            "doing" => 0,
            "open" => 1,
            "waiting" => 2,
            _ => 3,
        };
        v.sort_by(|a, b| (col(&a.status), a.priority).cmp(&(col(&b.status), b.priority)).then(a.rank.total_cmp(&b.rank)).then(a.id.cmp(&b.id)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_round_trip_and_questions_wait_on_the_raiser() {
        let dir = std::env::temp_dir().join(format!("topo_scan_issues_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::load(&dir).unwrap();
        let (steven, wrote) = s.participant("browser-1", "person").unwrap();
        assert!(wrote.is_some() && WORDS.contains(&steven.name.as_str()));
        let (claude, _) = s.participant("claude-1", "agent").unwrap();
        assert_ne!(steven.colour, claude.colour);
        s.rename("browser-1", "Steven").unwrap();

        let id = s.create("browser-1", "North wall short?", "scan", Some(Place::about([1.0, 14.0, 1.2], 0.1)), 1);
        s.update(id, "claude-1", Some("doing"), None, None, Some(Some("claude-1".into())), None).unwrap();
        s.comment(id, "claude-1", "Is this surface the sheathing?", Some(serde_json::json!({"colour": "normal"})), Some("North wall, normals".into())).unwrap();
        assert_eq!((s.get(id).unwrap().status.as_str(), s.get(id).unwrap().waiting_on.as_deref()), ("waiting", Some("browser-1")));
        s.comment(id, "browser-1", "Yes, between the studs.", None, None).unwrap();
        assert_eq!(s.get(id).unwrap().status, "doing");
        s.save(id).unwrap();

        // An agent's question in an issue of its own waits on any person.
        let own = s.create("claude-1", "T7's web looks plumb", "scan", None, 2);
        s.comment(own, "claude-1", "Is it plumb?", None, None).unwrap();
        assert_eq!((s.get(own).unwrap().status.as_str(), s.get(own).unwrap().waiting_on.clone()), ("waiting", None));
        s.comment(own, "claude-1", "(Still looking.)", None, None).unwrap();
        assert_eq!(s.get(own).unwrap().status, "waiting");
        s.comment(own, "browser-1", "Yes.", None, None).unwrap();
        assert_eq!(s.get(own).unwrap().status, "open");
        s.save(own).unwrap();

        let back = Store::load(&dir).unwrap();
        assert_eq!(back.issues, s.issues);
        assert_eq!(back.name_of("browser-1"), "Steven");
        assert_eq!(back.get(id).unwrap().entries.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn working_order_is_in_hand_then_open_by_priority() {
        let mut s = Store { dir: PathBuf::new(), issues: Vec::new(), participants: Vec::new() };
        let low = s.create("a", "low", "note", None, 3);
        let high = s.create("a", "high", "note", None, 1);
        let doing = s.create("a", "doing", "note", None, 2);
        let done = s.create("a", "done", "note", None, 1);
        s.update(doing, "a", Some("doing"), None, None, None, None).unwrap();
        s.update(done, "a", Some("done"), None, None, None, None).unwrap();
        assert_eq!(s.working_order().iter().map(|i| i.id).collect::<Vec<_>>(), vec![doing, high, low, done]);
    }
}
