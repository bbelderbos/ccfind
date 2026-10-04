pub mod agents;
pub mod render;

const SNIPPET_RADIUS: usize = 60;

/// Searchable view of one transcript: title, cwd, and the human/assistant prose (no tool output).
#[derive(Debug, Default, PartialEq)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub last_active: String,
    pub texts: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct Hit {
    /// Indices into the query terms this session contains.
    pub matched: Vec<usize>,
    pub snippet: String,
}

/// Finds which terms (ASCII-lowercased) a session contains; `None` if none.
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
    let matched: Vec<usize> = (0..terms.len()).filter(|&i| found(&terms[i])).collect();
    if matched.is_empty() {
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
    Some(Hit { matched, snippet })
}

/// Per-term IDF weights: a term found in few sessions counts more than one found in many.
pub fn term_weights(total_sessions: usize, hits: &[&Hit], term_count: usize) -> Vec<f64> {
    let mut doc_freq = vec![0usize; term_count];
    for i in hits.iter().flat_map(|hit| &hit.matched) {
        doc_freq[*i] += 1;
    }
    doc_freq
        .iter()
        .map(|&df| ((1 + total_sessions) as f64 / (1 + df) as f64).ln())
        .collect()
}

pub fn score(hit: &Hit, weights: &[f64]) -> f64 {
    hit.matched.iter().map(|&i| weights[i]).sum()
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

    fn session() -> Session {
        Session {
            title: Some("Rust talk feedback".into()),
            texts: vec![
                "give me feedback on my Rust preso transcript".into(),
                "The borrow checker section drags.".into(),
            ],
            ..Session::default()
        }
    }

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn scores_all_terms_across_messages() {
        let session = session();
        let hit = search(&session, &terms(&["preso", "borrow"])).unwrap();
        assert_eq!(hit.matched, vec![0, 1]);
    }

    #[test]
    fn partial_match_scores_lower() {
        let session = session();
        let hit = search(&session, &terms(&["preso", "python"])).unwrap();
        assert_eq!(hit.matched, vec![0]);
    }

    #[test]
    fn no_matching_term_is_no_hit() {
        let session = session();
        assert_eq!(search(&session, &terms(&["python", "django"])), None);
    }

    #[test]
    fn matches_title_terms() {
        let session = session();
        let hit = search(&session, &terms(&["talk"])).unwrap();
        assert_eq!(hit.snippet, "Rust talk feedback");
    }

    #[test]
    fn snippet_comes_from_message_with_most_terms() {
        let session = session();
        let hit = search(&session, &terms(&["rust", "transcript"])).unwrap();
        assert_eq!(hit.snippet, "give me feedback on my Rust preso transcript");
    }

    fn hit(matched: &[usize]) -> Hit {
        Hit {
            matched: matched.to_vec(),
            snippet: String::new(),
        }
    }

    #[test]
    fn one_rare_term_outranks_two_common_ones() {
        let common: Vec<Hit> = (0..8).map(|_| hit(&[0, 1])).collect();
        let rare = hit(&[2]);
        let all: Vec<&Hit> = common.iter().chain([&rare]).collect();
        let weights = term_weights(10, &all, 3);
        assert!(score(&rare, &weights) > score(&common[0], &weights));
    }

    #[test]
    fn matching_more_terms_scores_higher_at_equal_rarity() {
        let both = hit(&[0, 1]);
        let one = hit(&[0]);
        let weights = term_weights(10, &[&both, &one], 2);
        assert!(score(&both, &weights) > score(&one, &weights));
    }

    #[test]
    fn snippet_respects_multibyte_boundaries() {
        let text = format!("{}needle{}", "é".repeat(50), "ü".repeat(50));
        let s = snippet(&text, text.find("needle").unwrap());
        assert!(s.contains("needle"));
    }
}
