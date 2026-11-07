//! Subsequence and fuzzy matching for node search and library panels.

/// Case-insensitive fuzzy subsequence score (higher is better). Returns `None` if not a match.
///
/// Matching rules:
/// 1. An empty query matches everything with score 0.
/// 2. Every non-whitespace character in `query` must appear in `hay` in sequential order.
/// 3. Scoring rewards:
///    - Exact match bonus (+100)
///    - Exact prefix match bonus (+50)
///    - Word prefix match bonus (+40)
///    - Exact substring containment bonus (+30)
///    - Contiguous character matches (+8 per character)
///    - Word-boundary matches: start of string or right after `' '`, `'_'`, `'-'`, `'/'` (+6 per character)
/// 4. Length penalty: subtracts `(hay.len() / 16)` so shorter, tighter matches rank ahead of sprawling names.
pub fn fuzzy_score(hay: &str, query: &str) -> Option<i32> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        return Some(0);
    }

    let hay_lower = hay.to_ascii_lowercase();
    let q_lower = query_trimmed.to_ascii_lowercase();

    let words: Vec<&str> = q_lower.split_whitespace().collect();
    if words.len() > 1 {
        let mut total = 0i32;
        for w in words {
            total = total.saturating_add(single_word_score(&hay_lower, w)?);
        }
        return Some(total);
    }

    single_word_score(&hay_lower, &q_lower)
}

fn single_word_score(hay_lower: &str, q_lower: &str) -> Option<i32> {
    let mut score = 0i32;

    if hay_lower == q_lower {
        score = score.saturating_add(100);
    } else if hay_lower.starts_with(q_lower) {
        score = score.saturating_add(50);
    } else if hay_lower.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q_lower)) {
        score = score.saturating_add(40);
    } else if hay_lower.contains(q_lower) {
        score = score.saturating_add(30);
    }

    let mut last_idx: Option<usize> = None;
    let mut chars = hay_lower.char_indices();
    for q in q_lower.chars().filter(|c| !c.is_whitespace()) {
        let (idx, _) = chars.by_ref().find(|(_, c)| *c == q)?;
        let contiguous = match last_idx {
            Some(prev) if idx == prev.saturating_add(1) => 8,
            _ => 1,
        };
        score = score.saturating_add(contiguous);

        let is_boundary = idx == 0 || matches!(hay_lower.as_bytes().get(idx.saturating_sub(1)), Some(&b' ' | &b'_' | &b'-' | &b'/'));
        if is_boundary {
            score = score.saturating_add(6);
        }
        last_idx = Some(idx);
    }

    let len_penalty = (hay_lower.len() / 16) as i32;
    Some(score.saturating_sub(len_penalty))
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn test_empty_query_matches_with_zero() {
        assert_eq!(fuzzy_score("Gaussian Blur", ""), Some(0));
        assert_eq!(fuzzy_score("Gaussian Blur", "   "), Some(0));
    }

    #[test]
    fn test_acronym_and_subsequence_matching() {
        assert!(fuzzy_score("Gaussian Blur", "gblur").is_some());
        assert!(fuzzy_score("Transformation 2D", "t2d").is_some());
        assert!(fuzzy_score("Directional Warp", "dirwarp").is_some());
        assert!(fuzzy_score("Normal", "nrm").is_some());
        assert!(fuzzy_score("Grayscale Conversion", "grayconv").is_some());
    }

    #[test]
    fn test_non_matching_query_returns_none() {
        assert_eq!(fuzzy_score("Gaussian Blur", "xyz"), None);
        assert_eq!(fuzzy_score("Transformation 2D", "blur"), None);
    }

    #[test]
    fn test_prefix_and_exact_match_priority() {
        let blur_on_blur = fuzzy_score("Blur", "blur").unwrap_or(0);
        let blur_on_directional = fuzzy_score("Directional Warp", "blur").unwrap_or(0);
        assert!(blur_on_blur > blur_on_directional, "Exact/prefix 'Blur' ({blur_on_blur}) must score higher than 'Directional Warp' ({blur_on_directional})");
    }

    #[test]
    fn test_word_boundary_bonus() {
        // 'gb' in 'Gaussian Blur' matches 'G' and 'B' at word boundaries
        let boundary = fuzzy_score("Gaussian Blur", "gb").unwrap_or(0);
        // 'ub' in 'Gaussian Blur' matches 'u' in Gaussian and 'b' in Blur (only 1 boundary)
        let non_boundary = fuzzy_score("Gaussian Blur", "ub").unwrap_or(0);
        assert!(boundary > non_boundary);
    }

    #[test]
    fn test_multi_word_query() {
        assert!(fuzzy_score("Directional Warp", "warp dir").is_some());
        assert!(fuzzy_score("Anisotropic Noise", "noise aniso").is_some());
        assert_eq!(fuzzy_score("Directional Warp", "warp blur"), None);
    }
}
