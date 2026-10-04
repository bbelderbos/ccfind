use crate::{Hit, Session};

const TITLE_FALLBACK_LEN: usize = 70;

#[derive(Clone, Copy)]
pub struct Style {
    pub color: bool,
}

impl Style {
    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

const BOLD: &str = "1";
const DIM: &str = "2";
const CYAN: &str = "36";
const MATCH: &str = "1;33";

/// One result as a four-line block: rank + title, meta, highlighted snippet, resume command.
pub fn render_hit(
    rank: usize,
    session: &Session,
    hit: &Hit,
    terms: &[String],
    home: &str,
    style: Style,
) -> String {
    let date = session.last_active.get(..10).unwrap_or("?");
    let path = tilde(&session.cwd, home);
    format!(
        "{} {}\n    {}  {}\n    …{}…\n    {}\n",
        style.paint(DIM, &format!("{rank:>2}.")),
        style.paint(BOLD, &title(session)),
        style.paint(CYAN, &path),
        style.paint(
            DIM,
            &format!("{date} · {}/{} terms", hit.score, terms.len())
        ),
        highlight(&hit.snippet, terms, style),
        style.paint(DIM, &format!("cd {path} && claude --resume {}", session.id)),
    )
}

fn title(session: &Session) -> String {
    match (&session.title, session.texts.first()) {
        (Some(title), _) => title.clone(),
        (None, Some(first)) => {
            let line = first.split_whitespace().collect::<Vec<_>>().join(" ");
            match line.char_indices().nth(TITLE_FALLBACK_LEN) {
                Some((cut, _)) => format!("{}…", &line[..cut]),
                None => line,
            }
        }
        (None, None) => "(untitled)".to_string(),
    }
}

fn tilde(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_string(),
    }
}

fn highlight(text: &str, terms: &[String], style: Style) -> String {
    let lowered = text.to_ascii_lowercase();
    let mut ranges: Vec<(usize, usize)> = terms
        .iter()
        .filter(|t| !t.is_empty())
        .flat_map(|t| {
            lowered
                .match_indices(t.as_str())
                .map(|(i, m)| (i, i + m.len()))
        })
        .collect();
    ranges.sort_unstable();
    let merged = ranges
        .into_iter()
        .fold(Vec::<(usize, usize)>::new(), |mut acc, (start, end)| {
            match acc.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => acc.push((start, end)),
            }
            acc
        });
    let mut out = String::new();
    let mut pos = 0;
    for (start, end) in merged {
        out.push_str(&text[pos..start]);
        out.push_str(&style.paint(MATCH, &text[start..end]));
        pos = end;
    }
    out.push_str(&text[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR: Style = Style { color: true };
    const PLAIN: Style = Style { color: false };

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn highlights_terms_case_insensitively() {
        let out = highlight("Rust preso", &terms(&["rust"]), COLOR);
        assert_eq!(out, "\x1b[1;33mRust\x1b[0m preso");
    }

    #[test]
    fn merges_overlapping_matches() {
        let out = highlight("transcript", &terms(&["trans", "script"]), COLOR);
        assert_eq!(out, "\x1b[1;33mtranscript\x1b[0m");
    }

    #[test]
    fn plain_style_leaves_text_untouched() {
        assert_eq!(
            highlight("Rust preso", &terms(&["rust"]), PLAIN),
            "Rust preso"
        );
    }

    #[test]
    fn shortens_home_to_tilde_only_on_path_boundary() {
        assert_eq!(tilde("/Users/bob/code", "/Users/bob"), "~/code");
        assert_eq!(tilde("/Users/bobby", "/Users/bob"), "/Users/bobby");
        assert_eq!(tilde("/Users/bob", "/Users/bob"), "~");
    }

    #[test]
    fn untitled_session_falls_back_to_truncated_first_prompt() {
        let session = Session {
            texts: vec![format!("fix   the\nbug {}", "x".repeat(100))],
            ..Session::default()
        };
        let t = title(&session);
        assert!(t.starts_with("fix the bug x"));
        assert_eq!(t.chars().count(), TITLE_FALLBACK_LEN + 1);
    }

    #[test]
    fn renders_plain_block() {
        let session = Session {
            id: "abc".into(),
            cwd: "/Users/bob/Downloads".into(),
            title: Some("Transcribe talk".into()),
            last_active: "2026-10-04T10:00:00Z".into(),
            texts: vec![],
        };
        let hit = Hit {
            score: 1,
            snippet: "the transcript".into(),
        };
        let out = render_hit(
            1,
            &session,
            &hit,
            &terms(&["transcript", "rust"]),
            "/Users/bob",
            PLAIN,
        );
        assert_eq!(
            out,
            " 1. Transcribe talk\n    ~/Downloads  2026-10-04 · 1/2 terms\n    …the transcript…\n    cd ~/Downloads && claude --resume abc\n"
        );
    }
}
