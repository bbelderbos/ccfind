use serde::Deserialize;

pub mod render;

const SNIPPET_RADIUS: usize = 60;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Entry {
    User(Turn),
    Assistant(Turn),
    #[serde(rename_all = "camelCase")]
    AiTitle {
        ai_title: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Turn {
    message: Message,
    #[serde(default)]
    timestamp: String,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Content,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

/// Searchable view of one transcript: title, cwd, and the human/assistant prose (no tool output).
#[derive(Debug, Default, PartialEq)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub last_active: String,
    pub texts: Vec<String>,
}

/// Malformed or unknown lines are skipped: transcripts mix many record types we don't need.
pub fn parse_session(id: &str, jsonl: &str) -> Session {
    let mut session = Session {
        id: id.to_string(),
        ..Session::default()
    };
    for entry in jsonl
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
    {
        match entry {
            Entry::User(turn) | Entry::Assistant(turn) => add_turn(&mut session, turn),
            Entry::AiTitle { ai_title } => session.title = Some(ai_title),
            Entry::Other => {}
        }
    }
    session
}

fn add_turn(session: &mut Session, turn: Turn) {
    if session.cwd.is_empty()
        && let Some(cwd) = turn.cwd
    {
        session.cwd = cwd;
    }
    if turn.timestamp > session.last_active {
        session.last_active = turn.timestamp;
    }
    match turn.message.content {
        Content::Text(text) => session.texts.push(text),
        Content::Blocks(blocks) => {
            session
                .texts
                .extend(blocks.into_iter().filter_map(|b| match b {
                    Block::Text { text } => Some(text),
                    Block::Other => None,
                }))
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Hit {
    pub score: usize,
    pub snippet: String,
}

/// Scores a session by how many distinct terms (ASCII-lowercased) it contains; `None` if none.
pub fn search(session: &Session, terms: &[String]) -> Option<Hit> {
    let lowered: Vec<String> = session
        .texts
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let title = session
        .title
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let found = |term: &String| title.contains(term) || lowered.iter().any(|t| t.contains(term));
    let score = terms.iter().filter(|term| found(term)).count();
    if score == 0 {
        return None;
    }
    let term_count = |t: &String| terms.iter().filter(|term| t.contains(*term)).count();
    let best = lowered
        .iter()
        .enumerate()
        .max_by_key(|(i, t)| (term_count(t), std::cmp::Reverse(*i)));
    let snippet = match best {
        Some((i, text)) if term_count(text) > 0 => {
            let at = terms.iter().filter_map(|term| text.find(term)).min()?;
            snippet(&session.texts[i], at)
        }
        _ => session.title.clone()?,
    };
    Some(Hit { score, snippet })
}

fn snippet(text: &str, at: usize) -> String {
    let start = floor_char_boundary(text, at.saturating_sub(SNIPPET_RADIUS));
    let end = floor_char_boundary(text, (at + SNIPPET_RADIUS).min(text.len()));
    text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn floor_char_boundary(text: &str, mut i: usize) -> usize {
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = r#"{"type":"mode","mode":"normal"}
{"type":"user","message":{"role":"user","content":"give me feedback on my Rust preso transcript"},"timestamp":"2026-10-01T16:12:01Z","cwd":"/Users/pybob/code/shorts"}
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"borrow"},{"type":"text","text":"The borrow checker section drags."}]},"timestamp":"2026-10-01T16:13:00Z","cwd":"/Users/pybob/code/shorts"}
{"type":"user","message":{"content":[{"type":"tool_result","content":"secret tool output"}]},"timestamp":"2026-10-01T16:14:00Z"}
{"type":"ai-title","aiTitle":"Rust talk feedback"}
not json at all"#;

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn parses_prose_title_cwd_and_latest_timestamp() {
        let session = parse_session("abc", TRANSCRIPT);
        assert_eq!(session.cwd, "/Users/pybob/code/shorts");
        assert_eq!(session.title.as_deref(), Some("Rust talk feedback"));
        assert_eq!(session.last_active, "2026-10-01T16:14:00Z");
        assert_eq!(
            session.texts,
            vec![
                "give me feedback on my Rust preso transcript",
                "The borrow checker section drags."
            ]
        );
    }

    #[test]
    fn scores_all_terms_across_messages() {
        let session = parse_session("abc", TRANSCRIPT);
        let hit = search(&session, &terms(&["preso", "borrow"])).unwrap();
        assert_eq!(hit.score, 2);
    }

    #[test]
    fn partial_match_scores_lower() {
        let session = parse_session("abc", TRANSCRIPT);
        let hit = search(&session, &terms(&["preso", "python"])).unwrap();
        assert_eq!(hit.score, 1);
    }

    #[test]
    fn no_matching_term_is_no_hit() {
        let session = parse_session("abc", TRANSCRIPT);
        assert_eq!(search(&session, &terms(&["python", "django"])), None);
    }

    #[test]
    fn ignores_tool_output_and_thinking() {
        let session = parse_session("abc", TRANSCRIPT);
        assert_eq!(search(&session, &terms(&["secret"])), None);
    }

    #[test]
    fn matches_title_terms() {
        let session = parse_session("abc", TRANSCRIPT);
        let hit = search(&session, &terms(&["talk"])).unwrap();
        assert_eq!(hit.snippet, "Rust talk feedback");
    }

    #[test]
    fn snippet_comes_from_message_with_most_terms() {
        let session = parse_session("abc", TRANSCRIPT);
        let hit = search(&session, &terms(&["rust", "transcript"])).unwrap();
        assert_eq!(hit.snippet, "give me feedback on my Rust preso transcript");
    }

    #[test]
    fn snippet_respects_multibyte_boundaries() {
        let text = format!("{}needle{}", "é".repeat(50), "ü".repeat(50));
        let s = snippet(&text, text.find("needle").unwrap());
        assert!(s.contains("needle"));
    }
}
