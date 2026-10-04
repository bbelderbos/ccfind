use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs, io};

use ccfind::render::{Style, render_hit};
use ccfind::{Hit, Session, parse_session, search};
use rayon::prelude::*;

const MAX_RESULTS: usize = 10;

fn main() -> ExitCode {
    let terms: Vec<String> = env::args()
        .skip(1)
        .map(|a| a.to_ascii_lowercase())
        .collect();
    if terms.is_empty() {
        eprintln!("usage: ccfind <term>...   (ranked by terms matched, case-insensitive)");
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
    hits.sort_by(|(a, a_hit), (b, b_hit)| {
        (b_hit.score, &b.last_active).cmp(&(a_hit.score, &a.last_active))
    });

    let style = Style {
        color: io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
    };
    let home = home.to_string_lossy();
    let mut out = io::stdout().lock();
    for (rank, (session, hit)) in hits.iter().take(MAX_RESULTS).enumerate() {
        let block = render_hit(rank + 1, session, hit, &terms, &home, style);
        // A closed pipe (e.g. `| head`) just means the reader has seen enough.
        if writeln!(out, "{block}").is_err() {
            return ExitCode::SUCCESS;
        }
    }
    if hits.len() > MAX_RESULTS {
        let _ = writeln!(
            out,
            "… {} more, add terms to narrow",
            hits.len() - MAX_RESULTS
        );
    }
    if hits.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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
