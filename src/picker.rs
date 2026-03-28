use std::io::{self, BufRead, Write};

/// A scored match for fuzzy search.
#[derive(Debug, Clone)]
pub struct ScoredMatch {
    pub index: usize,
    pub text: String,
    pub score: i64,
    pub matched_indices: Vec<usize>,
}

/// Perform fuzzy matching of a query against a list of items.
/// Returns items sorted by score (best match first).
pub fn fuzzy_match(query: &str, items: &[String]) -> Vec<ScoredMatch> {
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(i, text)| ScoredMatch {
                index: i,
                text: text.clone(),
                score: 0,
                matched_indices: vec![],
            })
            .collect();
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();

    let mut matches: Vec<ScoredMatch> = items
        .iter()
        .enumerate()
        .filter_map(|(i, text)| {
            let (score, indices) = score_match(&query_lower, text);
            if score > 0 {
                Some(ScoredMatch {
                    index: i,
                    text: text.clone(),
                    score,
                    matched_indices: indices,
                })
            } else {
                None
            }
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score));
    matches
}

/// Score how well a query matches a text. Returns (score, matched_indices).
/// Returns (0, []) if there's no match.
fn score_match(query: &[char], text: &str) -> (i64, Vec<usize>) {
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();

    // Check if all query characters appear in order
    let mut indices = Vec::new();
    let mut text_idx = 0;

    for &qch in query {
        let mut found = false;
        while text_idx < text_lower.len() {
            if text_lower[text_idx] == qch {
                indices.push(text_idx);
                text_idx += 1;
                found = true;
                break;
            }
            text_idx += 1;
        }
        if !found {
            return (0, vec![]);
        }
    }

    // Score based on match quality
    let mut score: i64 = 100;

    // Bonus for exact substring match
    let text_lower_str: String = text_lower.iter().collect();
    let query_str: String = query.iter().collect();
    if text_lower_str.contains(&query_str) {
        score += 50;
    }

    // Bonus for prefix match
    if text_lower_str.starts_with(&query_str) {
        score += 30;
    }

    // Bonus for consecutive matches
    let mut consecutive_bonus = 0i64;
    for window in indices.windows(2) {
        if window[1] == window[0] + 1 {
            consecutive_bonus += 10;
        }
    }
    score += consecutive_bonus;

    // Bonus for matching at word boundaries (after _, ::, .)
    for &idx in &indices {
        if idx == 0 {
            score += 15;
        } else if let Some(&prev_ch) = text.as_bytes().get(idx - 1)
            && (prev_ch == b'_' || prev_ch == b':' || prev_ch == b'.' || prev_ch == b'/')
        {
            score += 10;
        }
    }

    // Penalty for spread-out matches
    if indices.len() >= 2 {
        let spread = indices.last().unwrap() - indices.first().unwrap();
        let min_spread = indices.len() - 1;
        let excess_spread = spread.saturating_sub(min_spread);
        score -= excess_spread as i64 * 2;
    }

    // Shorter matches are better (when query matches)
    score -= (text.len() as i64 - query.len() as i64).abs();

    score = score.max(1); // Ensure positive score if matched
    (score, indices)
}

/// Render matched text with highlighting using ANSI colors.
pub fn highlight_match(text: &str, matched_indices: &[usize]) -> String {
    if matched_indices.is_empty() {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut in_highlight = false;

    for (i, ch) in chars.iter().enumerate() {
        let is_matched = matched_indices.contains(&i);

        if is_matched && !in_highlight {
            result.push_str("\x1b[1;33m"); // Bold yellow
            in_highlight = true;
        } else if !is_matched && in_highlight {
            result.push_str("\x1b[0m"); // Reset
            in_highlight = false;
        }

        result.push(*ch);
    }

    if in_highlight {
        result.push_str("\x1b[0m");
    }

    result
}

/// Interactive test picker using stdin/stdout.
/// Returns the selected test names.
pub fn interactive_pick(test_names: &[String], prompt: &str) -> io::Result<Vec<String>> {
    if test_names.is_empty() {
        eprintln!("No tests available to pick from.");
        return Ok(vec![]);
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    eprintln!("{}", prompt);
    eprintln!("Type to filter, enter number(s) to select (comma-separated), 'q' to cancel:");
    eprintln!();

    // Show all tests initially
    display_items(test_names, &[], 20);

    eprint!("\n> ");
    stdout.flush()?;

    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    let input = line.trim();

    if input.eq_ignore_ascii_case("q") || input.is_empty() {
        return Ok(vec![]);
    }

    // Try to parse as numbers first
    let numbers: std::result::Result<Vec<usize>, _> = input
        .split(',')
        .map(|s| s.trim().parse::<usize>())
        .collect();

    if let Ok(nums) = numbers {
        let selected: Vec<String> = nums
            .into_iter()
            .filter(|&n| n > 0 && n <= test_names.len())
            .map(|n| test_names[n - 1].clone())
            .collect();
        return Ok(selected);
    }

    // Otherwise, treat as a filter pattern and show matches
    let matches = fuzzy_match(input, test_names);
    if matches.is_empty() {
        eprintln!("No tests match '{}'", input);
        return Ok(vec![]);
    }

    eprintln!("\nMatches for '{}':", input);
    for (i, m) in matches.iter().enumerate().take(20) {
        eprintln!(
            "  {:>3}. {}",
            i + 1,
            highlight_match(&m.text, &m.matched_indices)
        );
    }
    if matches.len() > 20 {
        eprintln!("  ... and {} more", matches.len() - 20);
    }

    eprint!("\nSelect number(s) or press Enter for all matches > ");
    stdout.flush()?;

    let mut line2 = String::new();
    stdin.lock().read_line(&mut line2)?;
    let input2 = line2.trim();

    if input2.is_empty() {
        // Return all matches
        return Ok(matches.into_iter().map(|m| m.text).collect());
    }

    if input2.eq_ignore_ascii_case("q") {
        return Ok(vec![]);
    }

    let nums: std::result::Result<Vec<usize>, _> = input2
        .split(',')
        .map(|s| s.trim().parse::<usize>())
        .collect();

    if let Ok(nums) = nums {
        let selected: Vec<String> = nums
            .into_iter()
            .filter(|&n| n > 0 && n <= matches.len())
            .map(|n| matches[n - 1].text.clone())
            .collect();
        Ok(selected)
    } else {
        eprintln!("Invalid selection.");
        Ok(vec![])
    }
}

fn display_items(items: &[String], matched: &[ScoredMatch], max: usize) {
    if matched.is_empty() {
        for (i, item) in items.iter().enumerate().take(max) {
            eprintln!("  {:>3}. {}", i + 1, item);
        }
    } else {
        for (i, m) in matched.iter().enumerate().take(max) {
            eprintln!(
                "  {:>3}. {}",
                i + 1,
                highlight_match(&m.text, &m.matched_indices)
            );
        }
    }

    let total = if matched.is_empty() {
        items.len()
    } else {
        matched.len()
    };
    if total > max {
        eprintln!("  ... and {} more", total - max);
    }
}

/// Non-interactive batch fuzzy filter: returns matching test names.
pub fn batch_fuzzy_filter(query: &str, test_names: &[String]) -> Vec<String> {
    let matches = fuzzy_match(query, test_names);
    matches.into_iter().map(|m| m.text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_empty_query() {
        let items = vec!["test_a".to_string(), "test_b".to_string()];
        let results = fuzzy_match("", &items);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn fuzzy_match_empty_items() {
        let results = fuzzy_match("query", &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn fuzzy_match_exact() {
        let items = vec![
            "test_alpha".to_string(),
            "test_beta".to_string(),
            "test_gamma".to_string(),
        ];
        let results = fuzzy_match("test_beta", &items);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "test_beta");
    }

    #[test]
    fn fuzzy_match_partial() {
        let items = vec![
            "test_connection_pool".to_string(),
            "test_database_query".to_string(),
            "test_cache_invalidation".to_string(),
        ];
        let results = fuzzy_match("conn", &items);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "test_connection_pool");
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        let items = vec!["TestAlpha".to_string(), "testBeta".to_string()];
        let results = fuzzy_match("alpha", &items);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "TestAlpha");
    }

    #[test]
    fn fuzzy_match_abbreviation() {
        let items = vec![
            "test_connection_pool_cleanup".to_string(),
            "test_everything_else".to_string(),
        ];
        let results = fuzzy_match("tcp", &items);
        // "tcp" should match "test_connection_pool_cleanup" via t, c, p
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "test_connection_pool_cleanup");
    }

    #[test]
    fn fuzzy_match_no_match() {
        let items = vec!["test_alpha".to_string()];
        let results = fuzzy_match("zzz", &items);
        assert!(results.is_empty());
    }

    #[test]
    fn fuzzy_match_ordering() {
        let items = vec![
            "something_unrelated".to_string(),
            "test_parse_output".to_string(),
            "parse".to_string(),
            "test_parse".to_string(),
        ];
        let results = fuzzy_match("parse", &items);
        // "parse" (exact match) should score highest
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "parse");
    }

    #[test]
    fn fuzzy_match_word_boundary_bonus() {
        let items = vec!["xyzparseabc".to_string(), "test_parse_output".to_string()];
        let results = fuzzy_match("parse", &items);
        // "test_parse_output" should score higher due to word boundary at _p
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text, "test_parse_output");
    }

    #[test]
    fn score_match_basic() {
        let query: Vec<char> = "abc".chars().collect();
        let (score, indices) = score_match(&query, "abc");
        assert!(score > 0);
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn score_match_no_match() {
        let query: Vec<char> = "xyz".chars().collect();
        let (score, _) = score_match(&query, "abc");
        assert_eq!(score, 0);
    }

    #[test]
    fn score_match_partial_order() {
        let query: Vec<char> = "ac".chars().collect();
        let (score, indices) = score_match(&query, "abc");
        assert!(score > 0);
        assert_eq!(indices, vec![0, 2]);
    }

    #[test]
    fn score_match_out_of_order_fails() {
        let query: Vec<char> = "ba".chars().collect();
        let (score, _) = score_match(&query, "abc");
        // 'b' at index 1, then 'a' needs to be after index 1 — not possible
        assert_eq!(score, 0);
    }

    #[test]
    fn highlight_match_basic() {
        let output = highlight_match("test_alpha", &[5, 6, 7, 8, 9]);
        assert!(output.contains("\x1b[1;33m")); // Contains highlight start
        assert!(output.contains("\x1b[0m")); // Contains reset
    }

    #[test]
    fn highlight_match_empty() {
        let output = highlight_match("test", &[]);
        assert_eq!(output, "test");
    }

    #[test]
    fn batch_fuzzy_filter_basic() {
        let items = vec![
            "test_alpha".to_string(),
            "test_beta".to_string(),
            "test_gamma".to_string(),
        ];
        let results = batch_fuzzy_filter("alpha", &items);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "test_alpha");
    }

    #[test]
    fn batch_fuzzy_filter_multiple() {
        let items = vec![
            "test_parse_json".to_string(),
            "test_parse_xml".to_string(),
            "test_format_json".to_string(),
        ];
        let results = batch_fuzzy_filter("parse", &items);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn batch_fuzzy_filter_empty_query() {
        let items = vec!["a".to_string(), "b".to_string()];
        let results = batch_fuzzy_filter("", &items);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn matched_indices_tracked() {
        let items = vec!["abcdef".to_string()];
        let results = fuzzy_match("ace", &items);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_indices, vec![0, 2, 4]);
    }

    #[test]
    fn prefix_match_scored_higher() {
        let items = vec!["zzz_test".to_string(), "test_zzz".to_string()];
        let results = fuzzy_match("test", &items);
        assert_eq!(results.len(), 2);
        // "test_zzz" starts with "test", should score higher
        assert_eq!(results[0].text, "test_zzz");
    }

    #[test]
    fn consecutive_matches_bonus() {
        let items = vec!["t_e_s_t".to_string(), "test_xyz".to_string()];
        let results = fuzzy_match("test", &items);
        assert_eq!(results.len(), 2);
        // "test_xyz" has consecutive matches for "test", should score higher
        assert_eq!(results[0].text, "test_xyz");
    }
}
