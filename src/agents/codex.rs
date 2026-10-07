use std::path::{Path, PathBuf};
use std::{fs, io};

use rust_json_parser::JsonValue;

use super::{Agent, leaf_strings, parse_object, take_string};
use crate::{Scope, Session};

/// Context Codex injects as user messages; not something the user typed.
const INJECTED_PREFIXES: &[&str] = &["<environment_context>", "# AGENTS.md instructions"];

pub struct Codex;

impl Agent for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    /// `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl`
    fn transcripts(&self, home: &Path) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        collect_jsonl(&home.join(".codex/sessions"), &mut files)?;
        Ok(files)
    }

    fn parse(&self, _path: &Path, contents: &str, scope: Scope) -> Session {
        let mut session = Session::default();
        // Only session_meta and response_item lines count towards last_active.
        for mut line in contents.lines().filter_map(parse_object) {
            match (
                take_string(&mut line, "type").as_deref(),
                line.remove("payload"),
            ) {
                (Some("session_meta"), Some(JsonValue::Object(mut meta))) => {
                    if let (Some(id), Some(cwd)) =
                        (take_string(&mut meta, "id"), take_string(&mut meta, "cwd"))
                    {
                        (session.id, session.cwd) = (id, cwd);
                    }
                }
                (Some("response_item"), Some(payload)) => add_item(&mut session, payload, scope),
                _ => continue,
            }
            if let Some(timestamp) = take_string(&mut line, "timestamp")
                && timestamp > session.last_active
            {
                session.last_active = timestamp;
            }
        }
        session
    }

    fn resume_command(&self, id: &str) -> String {
        format!("codex resume {id}")
    }
}

/// Messages are read field by field; any other item (tool calls, their output, reasoning)
/// is only searched in `Scope::Everything`, as raw strings.
fn add_item(session: &mut Session, payload: JsonValue, scope: Scope) {
    let JsonValue::Object(mut item) = payload else {
        return;
    };
    if item.get("type").and_then(JsonValue::as_str) != Some("message") {
        if scope == Scope::Everything {
            leaf_strings(JsonValue::Object(item), &mut session.texts);
        }
        return;
    }
    if take_string(&mut item, "role").is_none_or(|role| role == "developer") {
        return;
    }
    let Some(JsonValue::Array(parts)) = item.remove("content") else {
        return;
    };
    session.texts.extend(
        parts
            .into_iter()
            .filter_map(part_text)
            .filter(|text| !is_injected(text)),
    )
}

fn part_text(part: JsonValue) -> Option<String> {
    let JsonValue::Object(mut part) = part else {
        return None;
    };
    match take_string(&mut part, "type")?.as_str() {
        "input_text" | "output_text" => take_string(&mut part, "text"),
        _ => None,
    }
}

fn collect_jsonl(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_jsonl(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        }
    }
    Ok(())
}

fn is_injected(text: &str) -> bool {
    INJECTED_PREFIXES.iter().any(|p| text.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLLOUT: &str = r#"{"timestamp":"2026-01-18T08:59:25.057Z","type":"session_meta","payload":{"id":"019bd054","cwd":"/Users/pybob/code/shorts","timestamp":"2026-01-18T08:59:25.057Z","originator":"codex_cli_rs"}}
{"timestamp":"2026-01-18T08:59:25.075Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<permissions instructions>sandbox"}]}}
{"timestamp":"2026-01-18T08:59:25.075Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/Users/pybob</cwd>"}]}}
{"timestamp":"2026-01-18T09:00:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"transcribe my rust talk"}]}}
{"timestamp":"2026-01-18T09:00:05.000Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"secret"}}
{"timestamp":"2026-01-18T09:00:09.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done, saved to talk.txt"}]}}
{"timestamp":"2026-01-18T09:00:10.000Z","type":"event_msg","payload":{"type":"token_count"}}
not json"#;

    #[test]
    fn reads_id_and_cwd_from_session_meta() {
        let session = Codex.parse(Path::new("rollout.jsonl"), ROLLOUT, Scope::Conversation);
        assert_eq!(session.id, "019bd054");
        assert_eq!(session.cwd, "/Users/pybob/code/shorts");
        assert_eq!(session.last_active, "2026-01-18T09:00:09.000Z");
    }

    #[test]
    fn keeps_user_and_assistant_prose_only() {
        let session = Codex.parse(Path::new("rollout.jsonl"), ROLLOUT, Scope::Conversation);
        assert_eq!(
            session.texts,
            vec!["transcribe my rust talk", "Done, saved to talk.txt"]
        );
    }

    #[test]
    fn everything_scope_adds_tool_calls_but_not_injected_context() {
        let session = Codex.parse(Path::new("rollout.jsonl"), ROLLOUT, Scope::Everything);
        assert!(session.texts.iter().any(|t| t == "secret"));
        assert!(
            !session
                .texts
                .iter()
                .any(|t| t.contains("environment_context"))
        );
    }
}
