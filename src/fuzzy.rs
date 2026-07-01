/// Case-insensitive fuzzy subsequence matching.
///
/// Inspired by Codex/Pi: every query character must appear in order. Lower
/// scores are better; matches at path boundaries and contiguous runs score
/// ahead of spread-out matches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FuzzyMatch {
    pub indices: Vec<usize>,
    pub score: i32,
}

pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Some(FuzzyMatch { indices: Vec::new(), score: 0 });
    }

    let text_chars: Vec<char> = text.chars().collect();
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    if query_lower.len() > text_lower.len() {
        return None;
    }

    let mut query_idx = 0usize;
    let mut last_match: Option<usize> = None;
    let mut indices = Vec::with_capacity(query_lower.len());
    let mut score = 0i32;
    let mut contiguous_run = 0i32;

    for (idx, ch) in text_lower.iter().enumerate() {
        if query_idx >= query_lower.len() {
            break;
        }
        if *ch != query_lower[query_idx] {
            continue;
        }

        if Some(idx.saturating_sub(1)) == last_match {
            contiguous_run += 1;
            score -= contiguous_run * 5;
        } else {
            contiguous_run = 0;
            if let Some(prev) = last_match {
                score += (idx.saturating_sub(prev + 1) as i32) * 2;
            }
        }

        if is_path_boundary(&text_chars, idx) {
            score -= 10;
        }
        score += (idx as i32).min(50);

        indices.push(idx);
        last_match = Some(idx);
        query_idx += 1;
    }

    if query_idx != query_lower.len() {
        return None;
    }

    if text.eq_ignore_ascii_case(query) {
        score -= 100;
    }

    Some(FuzzyMatch { indices, score })
}

pub fn fuzzy_filter(items: &[String], query: &str, limit: usize) -> Vec<String> {
    let mut scored = items
        .iter()
        .filter_map(|item| fuzzy_match(query, item).map(|m| (m.score, item)))
        .collect::<Vec<_>>();
    scored.sort_by(|(a_score, a_item), (b_score, b_item)| a_score.cmp(b_score).then_with(|| a_item.cmp(b_item)));
    scored.into_iter().take(limit).map(|(_, item)| item.clone()).collect()
}

fn is_path_boundary(chars: &[char], idx: usize) -> bool {
    idx == 0
        || matches!(
            chars.get(idx.saturating_sub(1)),
            Some('/' | '\\' | '-' | '_' | '.' | ' ' | ':')
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_prefers_exact_and_boundary_matches() {
        let exact = fuzzy_match("app", "app").expect("match");
        let nested = fuzzy_match("app", "src/app.rs").expect("match");
        let spread = fuzzy_match("app", "alpha/path/provider.rs").expect("match");

        assert!(exact.score < nested.score);
        assert!(nested.score < spread.score);
    }

    #[test]
    fn fuzzy_filter_sorts_best_matches_first() {
        let items = vec![
            "src/provider/app.rs".to_string(),
            "src/app.rs".to_string(),
            "README.md".to_string(),
        ];

        assert_eq!(
            fuzzy_filter(&items, "app", 10),
            vec!["src/app.rs".to_string(), "src/provider/app.rs".to_string()]
        );
    }
}
