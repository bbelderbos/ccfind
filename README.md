# ccfind

Full-text search across all your Claude Code sessions, in every project directory.

`claude --resume` only searches titles. When you remember *what* you discussed but not *where* you ran it, ccfind finds the session and gives you the command to jump back in.

```console
$ ccfind mlx-whisper transcript
 1. Transcribe rust-cohort-weekly.mp4 with mlx-whisper
    ~/Downloads  2026-10-04 · 2/2 terms
    …use mlx-whisper to get a transcript for rust-cohort-weekly.mp4…
    cd ~/Downloads && claude --resume 280d6499-fcae-4c47-a673-76cfcd919a80
```

## Install

```bash
cargo install --path .
```

## Usage

```bash
ccfind <term>...
```

- Terms are case-insensitive substrings; a session doesn't need to contain all of them.
- Results are ranked by IDF: a term found in few sessions weighs more than one found in many, so a single rare word beats several common ones. Sessions matching every term still rank highest at equal rarity. Ties go to the most recent session; the top 10 are shown.
- Only your prompts, Claude's replies and the session title are searched. Tool output and thinking are skipped, so a session that merely *read* a file mentioning your term won't match.
- In a terminal, output goes through `$PAGER` (default `less`, with `LESS=FRX` unless you set `LESS`), so it starts at result 1 and short output is not paged. `PAGER=cat` turns paging off. Colors are on in a terminal and off when piped or when `NO_COLOR` is set.
- Exits 1 when nothing matches.

## How it works

Claude Code stores every session as a JSONL file under `~/.claude/projects/<encoded-dir>/<session-id>.jsonl`.

- **serde** deserializes each line into a tagged enum. Only `user`, `assistant` and `ai-title` records are kept, and unknown record types and malformed lines are skipped.
- **rayon** reads and parses the files in parallel: switching `.iter()` to `.par_iter()` is the only change from the sequential version. Searching about 500 sessions (≈500 MB) takes under a second.

The transcript format is internal to Claude Code and may change between versions.

## Development

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```
