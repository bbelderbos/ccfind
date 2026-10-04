use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Deserialize;

use super::Agent;
use crate::Session;

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

    fn parse(&self, _path: &Path, contents: &str) -> Session {
        let mut session = Session::default();
        for line in contents
            .lines()
            .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        {
            if line.timestamp > session.last_active {
                session.last_active = line.timestamp;
            }
            match line.record {
                Record::SessionMeta { id, cwd } => (session.id, session.cwd) = (id, cwd),
                Record::ResponseItem(Item::Message { role, content }) if role != "developer" => {
                    session
                        .texts
                        .extend(content.into_iter().filter_map(|part| match part {
                            Part::InputText { text } | Part::OutputText { text }
                                if !is_injected(&text) =>
                            {
                                Some(text)
                            }
                            _ => None,
                        }))
                }
                _ => {}
            }
        }
        session
    }

    fn resume_command(&self, id: &str) -> String {
        format!("codex resume {id}")
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

#[derive(Deserialize)]
struct Line {
    #[serde(default)]
    timestamp: String,
    #[serde(flatten)]
    record: Record,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum Record {
    SessionMeta {
        id: String,
        cwd: String,
    },
    ResponseItem(Item),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item {
    Message {
        role: String,
        content: Vec<Part>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Part {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    #[serde(other)]
    Other,
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
        let session = Codex.parse(Path::new("rollout.jsonl"), ROLLOUT);
        assert_eq!(session.id, "019bd054");
        assert_eq!(session.cwd, "/Users/pybob/code/shorts");
        assert_eq!(session.last_active, "2026-01-18T09:00:09.000Z");
    }

    #[test]
    fn keeps_user_and_assistant_prose_only() {
        let session = Codex.parse(Path::new("rollout.jsonl"), ROLLOUT);
        assert_eq!(
            session.texts,
            vec!["transcribe my rust talk", "Done, saved to talk.txt"]
        );
    }
}
