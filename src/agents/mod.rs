use std::io;
use std::path::{Path, PathBuf};

use crate::Session;

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
    /// Keep prose, title, cwd and latest timestamp; skip records you don't recognise.
    fn parse(&self, path: &Path, contents: &str) -> Session;
    /// Command that reopens session `id`, run from the session's cwd.
    fn resume_command(&self, id: &str) -> String;
}

pub const AGENTS: &[&dyn Agent] = &[&ClaudeCode, &Codex];
