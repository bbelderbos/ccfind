# ccfind

Full-text search across all your coding-agent sessions (Claude Code and Codex), in every project directory.

`claude --resume` only searches titles, and only one agent's. When you remember *what* you discussed but not *where* you ran it, ccfind finds the session and gives you the command to jump back in.

![ccfind mp4 subtitles finds matching sessions in three project directories, each with the command to resume it](https://raw.githubusercontent.com/bbelderbos/ccfind/main/docs/ccfind-search.png)

## Install

```bash
cargo install ccfind
```

## Usage

```bash
ccfind [--all] [--project <dir>] [--since <YYYY-MM-DD|Nd>] <term>...
```

- Terms are case-insensitive substrings; a session doesn't need to contain all of them.
- Results are ranked by IDF: a term found in few sessions weighs more than one found in many, so a single rare word beats several common ones. Sessions matching every term still rank highest at equal rarity. Ties go to the most recent session; the top 10 are shown.
- By default only your prompts, the agent's replies and the session title are searched. Tool output and thinking are skipped, so a session that merely *read* a file mentioning your term won't match.
- `--all` also searches tool calls, tool output and reasoning. Use it when what you remember was in a result, like an error message. Ids, signatures and base64 data are never searched, and neither is context the agent injects (memory, `CLAUDE.md`, `AGENTS.md`).
- `--project` keeps sessions whose directory contains the given text (case-insensitive), e.g. `--project ccfind`. `--since` keeps sessions active on or after a date (UTC, like the dates shown), given as `2026-10-01` or as days back, e.g. `7d`. Ranking is computed over the filtered sessions only.
- In a terminal, output goes through `$PAGER` (default `less`, with `LESS=FRX` unless you set `LESS`), so it starts at result 1 and short output is not paged. `PAGER=cat` turns paging off. Colors are on in a terminal and off when piped or when `NO_COLOR` is set.
- Exits 1 when nothing matches, 2 on a usage error.

## How it works

Each agent writes its sessions as JSONL files:

| Agent | Transcripts | Resume |
|---|---|---|
| Claude Code | `~/.claude/projects/<encoded-dir>/<session-id>.jsonl` | `claude --resume <id>` |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `codex resume <id>` |

- **[bob-json-parser](https://crates.io/crates/bob-json-parser)**, a hand-written JSON parser built in the [Scripter to Rust](https://scriptertorust.com) cohort, parses each line into a `JsonValue`. Each agent picks out prompts, replies and titles by key; tool payloads are flattened to their strings only for `--all`. Unknown record types and malformed lines are skipped.
- **rayon** reads and parses every agent's files in one parallel pass: switching `.iter()` to `.par_iter()` is the only change from the sequential version. Searching about 500 sessions (≈500 MB) takes under a second.
- Ranking is computed across all agents together, so results from different agents compete on the same scale.

These transcript formats are internal to each agent and may change between versions.

## Adding an agent

Each agent is one file in `src/agents/` that implements the `Agent` trait:

```rust
pub trait Agent: Sync {
    fn name(&self) -> &'static str;
    fn transcripts(&self, home: &Path) -> io::Result<Vec<PathBuf>>;
    fn parse(&self, path: &Path, contents: &str, scope: Scope) -> Session;
    fn resume_command(&self, id: &str) -> String;
}
```

1. Copy `src/agents/codex.rs` and adapt the field lookups to your agent's format.
2. Add fixture tests that parse a few real (scrubbed) lines, in both scopes. For `Scope::Everything`, pass tool payloads to `leaf_strings`.
3. Register it in `AGENTS` in `src/agents/mod.rs`.

Search, ranking, rendering and paging need no changes. An agent that isn't installed (`transcripts` returns `NotFound`) is skipped quietly.

## Development

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```
