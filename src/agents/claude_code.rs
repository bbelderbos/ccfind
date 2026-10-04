use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Deserialize;
use serde_json::Value;

use super::{Agent, leaf_strings};
use crate::{Scope, Session};

pub struct ClaudeCode;

impl Agent for ClaudeCode {
    fn name(&self) -> &'static str {
        "claude"
    }

    /// `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
    fn transcripts(&self, home: &Path) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for project in fs::read_dir(home.join(".claude/projects"))? {
            let project = project?.path();
            if !project.is_dir() {
                continue;
            }
            for file in fs::read_dir(&project)? {
                let file = file?.path();
                if file.extension().is_some_and(|ext| ext == "jsonl") {
                    files.push(file);
                }
            }
        }
        Ok(files)
    }

    fn parse(&self, path: &Path, contents: &str, scope: Scope) -> Session {
        let id = path.file_stem().unwrap_or_default().to_string_lossy();
        parse_session(&id, contents, scope)
    }

    fn resume_command(&self, id: &str) -> String {
        format!("claude --resume {id}")
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Entry {
    User(Turn),
    Assistant(Turn),
    #[serde(rename_all = "camelCase")]
    AiTitle {
        ai_title: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Turn {
    message: Message,
    #[serde(default)]
    timestamp: String,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Content,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        input: Value,
    },
    ToolResult {
        #[serde(default)]
        content: Value,
    },
    #[serde(other)]
    Other,
}

/// Malformed or unknown lines are skipped: transcripts mix many record types we don't need.
fn parse_session(id: &str, jsonl: &str, scope: Scope) -> Session {
    let mut session = Session {
        id: id.to_string(),
        ..Session::default()
    };
    for entry in jsonl
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
    {
        match entry {
            Entry::User(turn) | Entry::Assistant(turn) => add_turn(&mut session, turn, scope),
            Entry::AiTitle { ai_title } => session.title = Some(ai_title),
            Entry::Other => {}
        }
    }
    session
}

fn add_turn(session: &mut Session, turn: Turn, scope: Scope) {
    if session.cwd.is_empty()
        && let Some(cwd) = turn.cwd
    {
        session.cwd = cwd;
    }
    if turn.timestamp > session.last_active {
        session.last_active = turn.timestamp;
    }
    match turn.message.content {
        Content::Text(text) => session.texts.push(text),
        Content::Blocks(blocks) => {
            for block in blocks {
                match (block, scope) {
                    (Block::Text { text }, _) => session.texts.push(text),
                    (Block::Thinking { thinking }, Scope::Everything) => {
                        session.texts.push(thinking)
                    }
                    (Block::ToolUse { input: value }, Scope::Everything)
                    | (Block::ToolResult { content: value }, Scope::Everything) => {
                        leaf_strings(value, &mut session.texts)
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = r#"{"type":"mode","mode":"normal"}
{"type":"user","message":{"role":"user","content":"give me feedback on my Rust preso transcript"},"timestamp":"2026-10-01T16:12:01Z","cwd":"/Users/pybob/code/shorts"}
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"borrow"},{"type":"text","text":"The borrow checker section drags."}]},"timestamp":"2026-10-01T16:13:00Z","cwd":"/Users/pybob/code/shorts"}
{"type":"user","message":{"content":[{"type":"tool_result","content":"secret tool output"}]},"timestamp":"2026-10-01T16:14:00Z"}
{"type":"ai-title","aiTitle":"Rust talk feedback"}
not json at all"#;

    #[test]
    fn parses_prose_title_cwd_and_latest_timestamp() {
        let session = ClaudeCode.parse(Path::new("/x/abc.jsonl"), TRANSCRIPT, Scope::Conversation);
        assert_eq!(session.id, "abc");
        assert_eq!(session.cwd, "/Users/pybob/code/shorts");
        assert_eq!(session.title.as_deref(), Some("Rust talk feedback"));
        assert_eq!(session.last_active, "2026-10-01T16:14:00Z");
        assert_eq!(
            session.texts,
            vec![
                "give me feedback on my Rust preso transcript",
                "The borrow checker section drags."
            ]
        );
    }

    #[test]
    fn skips_tool_output_and_thinking() {
        let session = ClaudeCode.parse(Path::new("abc.jsonl"), TRANSCRIPT, Scope::Conversation);
        assert!(!session.texts.iter().any(|t| t.contains("secret")));
    }

    #[test]
    fn everything_scope_adds_tool_output_and_thinking() {
        let session = ClaudeCode.parse(Path::new("abc.jsonl"), TRANSCRIPT, Scope::Everything);
        assert!(session.texts.iter().any(|t| t == "secret tool output"));
        assert!(session.texts.iter().any(|t| t == "borrow"));
    }
}
