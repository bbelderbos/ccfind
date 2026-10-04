use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs, io};

use ccfind::{Session, parse_session, search};
use rayon::prelude::*;

const MAX_RESULTS: usize = 10;

fn main() -> ExitCode {
    let terms: Vec<String> = env::args()
        .skip(1)
        .map(|a| a.to_ascii_lowercase())
        .collect();
    if terms.is_empty() {
        eprintln!("usage: ccfind <term>...   (all terms must match, case-insensitive)");
        return ExitCode::from(2);
    }
    let Some(home) = env::var_os("HOME") else {
        eprintln!("ccfind: HOME is not set");
        return ExitCode::FAILURE;
    };
    let root = PathBuf::from(home).join(".claude/projects");
    let files = match transcript_files(&root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("ccfind: cannot read {}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };

    let mut hits: Vec<(Session, String)> = files
        .par_iter()
        .filter_map(|path| {
            let jsonl = fs::read_to_string(path)
                .inspect_err(|e| eprintln!("ccfind: skipping {}: {e}", path.display()))
                .ok()?;
            let session = parse_session(&file_stem(path), &jsonl);
            let snippet = search(&session, &terms)?;
            Some((session, snippet))
        })
        .collect();
    hits.sort_by(|a, b| b.0.last_active.cmp(&a.0.last_active));

    for (session, snippet) in hits.iter().take(MAX_RESULTS) {
        print_hit(session, snippet);
    }
    if hits.len() > MAX_RESULTS {
        println!("… {} more, add terms to narrow", hits.len() - MAX_RESULTS);
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

fn print_hit(session: &Session, snippet: &str) {
    let date = session.last_active.get(..10).unwrap_or("?");
    let title = session.title.as_deref().unwrap_or("(untitled)");
    println!("{date}  {}  {title}", session.cwd);
    println!("    …{snippet}…");
    println!("    cd {} && claude --resume {}\n", session.cwd, session.id);
}
