//! Chat: messages from one participant to another (a person to an agent, an
//! agent to a person, people to each other), outside any issue. For asking
//! and saying things that are not about a place or a decision; what is worth
//! keeping is raised as an issue.
//!
//! Messages are kept beside the repository's history (in its git directory),
//! not in it: they survive a restart, and are not commits.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::issues::now;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub time: f64,
}

#[derive(Debug, Default)]
pub struct Chat {
    path: Option<PathBuf>,
    pub messages: Vec<Message>,
}

impl Chat {
    pub fn open(path: PathBuf) -> Chat {
        let messages = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Chat { path: Some(path), messages }
    }

    fn save(&self) -> Result<()> {
        if let Some(p) = &self.path {
            std::fs::write(p, serde_json::to_string(&self.messages)?).with_context(|| format!("writing {}", p.display()))?;
        }
        Ok(())
    }

    /// The messages to `who` after `since`, from others.
    pub fn to(&self, who: &str, since: f64) -> impl Iterator<Item = &Message> {
        let who = who.to_string();
        self.messages.iter().filter(move |m| m.to == who && m.time > since)
    }

    /// When `who` last said anything here.
    pub fn last_from(&self, who: &str) -> f64 {
        self.messages.iter().filter(|m| m.from == who).map(|m| m.time).fold(0.0, f64::max)
    }
}

impl Workspace {
    /// A participant by id or by name.
    fn participant_id(&self, s: &str) -> Result<String> {
        let ps = &self.store.participants;
        ps.iter().find(|p| p.id == s).or_else(|| ps.iter().find(|p| p.name.eq_ignore_ascii_case(s))).map(|p| p.id.clone()).with_context(|| format!("nobody here is called {s:?}: {}", ps.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))
    }

    pub(crate) fn chat_tool(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let text = |s: String| Ok(vec![json!({"type": "text", "text": s})]);
        let line = |ws: &Self, m: &Message| format!("  {} → {}: {}\n", ws.store.name_of(&m.from), ws.store.name_of(&m.to), m.text);
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("send") {
            "send" => {
                let to = self.participant_id(args.get("to").and_then(|v| v.as_str()).context("to: who it is for (a name)")?)?;
                let body = args.get("text").and_then(|v| v.as_str()).map(str::trim).filter(|t| !t.is_empty()).context("text")?;
                if to == who {
                    bail!("that is you");
                }
                self.chat.messages.push(Message { from: who.to_string(), to: to.clone(), text: body.to_string(), time: now() });
                self.chat.save()?;
                self.touch();
                text(format!("sent to {}", self.store.name_of(&to)))
            }
            // What was said between you and them (or, without `with`, everything to or from you), the latest last.
            "read" => {
                let with = args.get("with").and_then(|v| v.as_str()).map(|w| self.participant_id(w)).transpose()?;
                let n = args.get("last").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
                let all: Vec<&Message> = self.chat.messages.iter().filter(|m| (m.from == who || m.to == who) && with.as_ref().is_none_or(|w| &m.from == w || &m.to == w)).collect();
                let s: String = all[all.len().saturating_sub(n)..].iter().map(|m| line(self, m)).collect();
                text(if s.is_empty() { "nothing said yet".into() } else { s })
            }
            a => bail!("action must be send or read, not {a:?}"),
        }
    }
}
