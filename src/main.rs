use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::{env, fs, io};

use ccfind::render::{Style, render_hit};
use ccfind::{Hit, Session, parse_session, score, search, term_weights};
use rayon::prelude::*;

const MAX_RESULTS: usize = 10;

fn main() -> ExitCode {
    let terms: Vec<String> = env::args()
        .skip(1)
        .map(|a| a.to_ascii_lowercase())
        .collect();
    if terms.is_empty() {
        eprintln!("usage: ccfind <term>...   (rare terms rank higher, case-insensitive)");
        return ExitCode::from(2);
    }
    let Some(home) = env::var_os("HOME") else {
        eprintln!("ccfind: HOME is not set");
        return ExitCode::FAILURE;
    };
    let root = PathBuf::from(&home).join(".claude/projects");
    let files = match transcript_files(&root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("ccfind: cannot read {}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };

    let mut hits: Vec<(Session, Hit)> = files
        .par_iter()
        .filter_map(|path| {
            let jsonl = fs::read_to_string(path)
                .inspect_err(|e| eprintln!("ccfind: skipping {}: {e}", path.display()))
                .ok()?;
            let session = parse_session(&file_stem(path), &jsonl);
            let hit = search(&session, &terms)?;
            Some((session, hit))
        })
        .collect();
    let only_hits: Vec<&Hit> = hits.iter().map(|(_, hit)| hit).collect();
    let weights = term_weights(files.len(), &only_hits, terms.len());
    hits.sort_by(|(a, a_hit), (b, b_hit)| {
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
    hits: &[(Session, Hit)],
    terms: &[String],
    home: &str,
    style: Style,
) -> io::Result<()> {
    for (rank, (session, hit)) in hits.iter().take(MAX_RESULTS).enumerate() {
        writeln!(
            out,
            "{}",
            render_hit(rank + 1, session, hit, terms, home, style)
        )?;
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

fn transcript_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for project in fs::read_dir(root)? {
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

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
