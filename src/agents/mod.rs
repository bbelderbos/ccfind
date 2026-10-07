use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use rust_json_parser::JsonValue;

use crate::{Scope, Session};

mod claude_code;
mod codex;

pub use claude_code::ClaudeCode;
pub use codex::Codex;

/// A coding agent whose transcripts ccfind can search. Implement it and add it to [`AGENTS`].
pub trait Agent: Sync {
    /// Short label shown next to each result.
    fn name(&self) -> &'static str;
    /// Every transcript file this agent has written under `home`.
    fn transcripts(&self, home: &Path) -> io::Result<Vec<PathBuf>>;
    /// Keep prose, title, cwd and latest timestamp (plus tool content for `Scope::Everything`);
    /// skip records you don't recognise.
    fn parse(&self, path: &Path, contents: &str, scope: Scope) -> Session;
    /// Command that reopens session `id`, run from the session's cwd.
    fn resume_command(&self, id: &str) -> String;
}

pub const AGENTS: &[&dyn Agent] = &[&ClaudeCode, &Codex];

/// Ids, signatures and base64 payloads: never useful to search, and they'd cause junk matches.
const SKIPPED_KEYS: &[&str] = &[
    "type",
    "id",
    "call_id",
    "tool_use_id",
    "signature",
    "data",
    "encrypted_content",
];

type Object = HashMap<String, JsonValue>;

/// A JSONL line as an object; `None` for malformed lines or non-object values.
fn parse_object(line: &str) -> Option<Object> {
    match rust_json_parser::parse(line) {
        Ok(JsonValue::Object(object)) => Some(object),
        _ => None,
    }
}

fn take_string(object: &mut Object, key: &str) -> Option<String> {
    match object.remove(key) {
        Some(JsonValue::String(s)) => Some(s),
        _ => None,
    }
}

/// Every string in a JSON value (tool input/output), minus the [`SKIPPED_KEYS`] fields.
fn leaf_strings(value: JsonValue, out: &mut Vec<String>) {
    match value {
        JsonValue::String(s) => out.push(s),
        JsonValue::Array(items) => items.into_iter().for_each(|v| leaf_strings(v, out)),
        JsonValue::Object(map) => map
            .into_iter()
            .filter(|(key, _)| !SKIPPED_KEYS.contains(&key.as_str()))
            .for_each(|(_, v)| leaf_strings(v, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_strings_except_skipped_keys() -> rust_json_parser::Result<()> {
        let value = rust_json_parser::parse(
            r#"{
                "tool_use_id": "toolu_123",
                "content": [{"type": "text", "text": "thread panicked"}],
                "input": {"command": "cargo test", "timeout": 120},
                "signature": "c2lnbmF0dXJl"
            }"#,
        )?;
        let mut out = Vec::new();
        leaf_strings(value, &mut out);
        out.sort();
        assert_eq!(out, vec!["cargo test", "thread panicked"]);
        Ok(())
    }
}
