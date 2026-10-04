use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::{env, fs, io};

use ccfind::agents::{AGENTS, Agent};
use ccfind::render::{Style, render_hit};
use ccfind::{Hit, Session, USAGE, parse_args, score, search, term_weights};
use rayon::prelude::*;

const MAX_RESULTS: usize = 10;

type Found = (&'static dyn Agent, Session, Hit);

fn main() -> ExitCode {
    let (terms, scope) = match parse_args(env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("ccfind: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some(home) = env::var_os("HOME") else {
        eprintln!("ccfind: HOME is not set");
        return ExitCode::FAILURE;
    };
    let files = all_transcripts(Path::new(&home));
    if files.is_empty() {
        eprintln!("ccfind: no transcripts found");
        return ExitCode::FAILURE;
    }

    let mut hits: Vec<Found> = files
        .par_iter()
        .filter_map(|&(agent, ref path)| {
            let jsonl = fs::read_to_string(path)
                .inspect_err(|e| eprintln!("ccfind: skipping {}: {e}", path.display()))
                .ok()?;
            let session = agent.parse(path, &jsonl, scope);
            let hit = search(&session, &terms)?;
            Some((agent, session, hit))
        })
        .collect();
    let only_hits: Vec<&Hit> = hits.iter().map(|(_, _, hit)| hit).collect();
    let weights = term_weights(files.len(), &only_hits, terms.len());
    hits.sort_by(|(_, a, a_hit), (_, b, b_hit)| {
        score(b_hit, &weights)
            .total_cmp(&score(a_hit, &weights))
            .then_with(|| b.last_active.cmp(&a.last_active))
    });

    let tty = io::stdout().is_terminal();
    let style = Style {
        color: tty && env::var_os("NO_COLOR").is_none(),
    };
    let mut pager = if tty && !hits.is_empty() {
        spawn_pager()
    } else {
        None
    };
    let mut out: Box<dyn Write> = match pager.as_mut().and_then(|p| p.stdin.take()) {
        Some(stdin) => Box::new(stdin),
        None => Box::new(io::stdout().lock()),
    };
    // A closed pipe (`| head`, or quitting the pager early) just means the reader has seen enough.
    let _ = write_results(&mut out, &hits, &terms, &home.to_string_lossy(), style);
    drop(out);
    if let Some(mut pager) = pager {
        let _ = pager.wait();
    }

    if hits.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn write_results(
    out: &mut dyn Write,
    hits: &[Found],
    terms: &[String],
    home: &str,
    style: Style,
) -> io::Result<()> {
    for (rank, (agent, session, hit)) in hits.iter().take(MAX_RESULTS).enumerate() {
        let block = render_hit(rank + 1, *agent, session, hit, terms, home, style);
        writeln!(out, "{block}")?;
    }
    if hits.len() > MAX_RESULTS {
        let more = hits.len() - MAX_RESULTS;
        writeln!(out, "… {more} more, add terms to narrow")?;
    }
    Ok(())
}

/// Like git: `$PAGER` (default less), with `LESS=FRX` unless set, so short output isn't paged.
fn spawn_pager() -> Option<Child> {
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    if pager.trim().is_empty() || pager == "cat" {
        return None;
    }
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &pager]).stdin(Stdio::piped());
    if env::var_os("LESS").is_none() {
        Command::env(&mut cmd, "LESS", "FRX");
    }
    cmd.spawn()
        .inspect_err(|e| eprintln!("ccfind: pager `{pager}` failed: {e}"))
        .ok()
}

/// Agents that aren't installed (no transcript dir) are skipped quietly.
fn all_transcripts(home: &Path) -> Vec<(&'static dyn Agent, PathBuf)> {
    let mut files = Vec::new();
    for &agent in AGENTS {
        match agent.transcripts(home) {
            Ok(paths) => files.extend(paths.into_iter().map(|p| (agent, p))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("ccfind: skipping {} transcripts: {e}", agent.name()),
        }
    }
    files
}
