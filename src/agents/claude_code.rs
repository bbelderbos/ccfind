use std::path::{Path, PathBuf};
use std::{fs, io};

use rust_json_parser::JsonValue;

use super::{Agent, Object, leaf_strings, parse_object, take_string};
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

/// Malformed or unknown lines are skipped: transcripts mix many record types we don't need.
fn parse_session(id: &str, jsonl: &str, scope: Scope) -> Session {
    let mut session = Session {
        id: id.to_string(),
        ..Session::default()
    };
    for mut entry in jsonl.lines().filter_map(parse_object) {
        match take_string(&mut entry, "type").as_deref() {
            Some("user" | "assistant") => add_turn(&mut session, entry, scope),
            Some("ai-title") => {
                if let Some(title) = take_string(&mut entry, "aiTitle") {
                    session.title = Some(title);
                }
            }
            _ => {}
        }
    }
    session
}

fn add_turn(session: &mut Session, mut turn: Object, scope: Scope) {
    if session.cwd.is_empty()
        && let Some(cwd) = take_string(&mut turn, "cwd")
    {
        session.cwd = cwd;
    }
    if let Some(timestamp) = take_string(&mut turn, "timestamp")
        && timestamp > session.last_active
    {
        session.last_active = timestamp;
    }
    let content = match turn.remove("message") {
        Some(JsonValue::Object(mut message)) => message.remove("content"),
        _ => None,
    };
    match content {
        Some(JsonValue::String(text)) => session.texts.push(text),
        Some(JsonValue::Array(blocks)) => blocks
            .into_iter()
            .for_each(|block| add_block(session, block, scope)),
        _ => {}
    }
}

fn add_block(session: &mut Session, block: JsonValue, scope: Scope) {
    let JsonValue::Object(mut block) = block else {
        return;
    };
    let field = match (take_string(&mut block, "type").as_deref(), scope) {
        (Some("text"), _) => "text",
        (Some("thinking"), Scope::Everything) => "thinking",
        (Some("tool_use"), Scope::Everything) => "input",
        (Some("tool_result"), Scope::Everything) => "content",
        _ => return,
    };
    if let Some(value) = block.remove(field) {
        leaf_strings(value, &mut session.texts);
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
