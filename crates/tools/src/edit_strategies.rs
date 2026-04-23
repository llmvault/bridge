//! Edit matching strategies inspired by OpenCode.
//!
//! When `apply_edit()` tries to find `old_string` in `content`, it uses a
//! chain of increasingly fuzzy strategies. The first match wins.

/// Trait for replacement strategies.
pub(crate) trait Replacer {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    /// Try to replace `old_string` with `new_string` in `content`.
    /// Returns `Some(new_content)` on success, `None` if the strategy cannot match.
    fn try_replace(
        &self,
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Option<(String, usize)>;
}

/// Return the ordered list of all strategies.
// Trimmed from 9 to 3: fuzzier strategies silently mis-edited code when the
// LLM supplied slightly wrong text. The LLM now retries with exact text.
pub(crate) fn all_strategies() -> Vec<Box<dyn Replacer>> {
    vec![
        Box::new(SimpleReplacer),
        Box::new(LineTrimmedReplacer),
        Box::new(WhitespaceNormalizedReplacer),
    ]
}

// ---------------------------------------------------------------------------
// 1. SimpleReplacer — exact string match
// ---------------------------------------------------------------------------

struct SimpleReplacer;

impl Replacer for SimpleReplacer {
    fn name(&self) -> &str {
        "simple"
    }

    fn try_replace(
        &self,
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Option<(String, usize)> {
        let count = content.matches(old_string).count();
        if count == 0 {
            return None;
        }
        if count > 1 && !replace_all {
            return None; // Ambiguous — let caller handle the error
        }
        if replace_all {
            Some((content.replace(old_string, new_string), count))
        } else {
            Some((content.replacen(old_string, new_string, 1), 1))
        }
    }
}

// ---------------------------------------------------------------------------
// 2. LineTrimmedReplacer — trim each line before matching
// ---------------------------------------------------------------------------

struct LineTrimmedReplacer;

impl Replacer for LineTrimmedReplacer {
    fn name(&self) -> &str {
        "line_trimmed"
    }

    fn try_replace(
        &self,
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Option<(String, usize)> {
        let old_lines: Vec<&str> = old_string.lines().map(|l| l.trim()).collect();
        let content_lines: Vec<&str> = content.lines().collect();
        let content_trimmed: Vec<&str> = content_lines.iter().map(|l| l.trim()).collect();

        if old_lines.is_empty() {
            return None;
        }

        let mut matches: Vec<usize> = Vec::new();
        for i in 0..=content_trimmed.len().saturating_sub(old_lines.len()) {
            if content_trimmed[i..i + old_lines.len()] == old_lines[..] {
                matches.push(i);
            }
        }

        if matches.is_empty() {
            return None;
        }
        if matches.len() > 1 && !replace_all {
            return None;
        }

        let new_lines_vec: Vec<&str> = new_string.lines().collect();
        let matches_to_apply = if replace_all {
            matches.clone()
        } else {
            vec![matches[0]]
        };

        let mut result_lines: Vec<&str> = content_lines;
        let mut sorted_matches = matches_to_apply.clone();
        sorted_matches.sort_unstable_by(|a, b| b.cmp(a));

        for start in &sorted_matches {
            let end = start + old_lines.len();
            let mut new_result: Vec<&str> = Vec::new();
            new_result.extend_from_slice(&result_lines[..*start]);
            new_result.extend_from_slice(&new_lines_vec);
            new_result.extend_from_slice(&result_lines[end..]);
            result_lines = new_result;
        }

        let mut new_content = result_lines.join("\n");
        if content.ends_with('\n') && !new_content.ends_with('\n') {
            new_content.push('\n');
        }

        Some((new_content, matches_to_apply.len()))
    }
}

// ---------------------------------------------------------------------------
// 3. WhitespaceNormalizedReplacer — collapse all whitespace to single spaces
// ---------------------------------------------------------------------------

struct WhitespaceNormalizedReplacer;

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Replacer for WhitespaceNormalizedReplacer {
    fn name(&self) -> &str {
        "whitespace_normalized"
    }

    fn try_replace(
        &self,
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Option<(String, usize)> {
        let norm_old = normalize_whitespace(old_string);
        let content_lines: Vec<&str> = content.lines().collect();
        let old_lines: Vec<&str> = old_string.lines().collect();

        if old_lines.is_empty() {
            return None;
        }

        // Find blocks where whitespace-normalized content matches
        let mut matches: Vec<usize> = Vec::new();
        for i in 0..=content_lines.len().saturating_sub(old_lines.len()) {
            let block = content_lines[i..i + old_lines.len()].join("\n");
            if normalize_whitespace(&block) == norm_old {
                matches.push(i);
            }
        }

        if matches.is_empty() {
            return None;
        }
        if matches.len() > 1 && !replace_all {
            return None;
        }

        let new_lines_vec: Vec<&str> = new_string.lines().collect();
        let matches_to_apply = if replace_all {
            matches.clone()
        } else {
            vec![matches[0]]
        };

        let mut result_lines: Vec<&str> = content_lines;
        let mut sorted = matches_to_apply.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));

        for start in &sorted {
            let end = start + old_lines.len();
            let mut new_result: Vec<&str> = Vec::new();
            new_result.extend_from_slice(&result_lines[..*start]);
            new_result.extend_from_slice(&new_lines_vec);
            new_result.extend_from_slice(&result_lines[end..]);
            result_lines = new_result;
        }

        let mut new_content = result_lines.join("\n");
        if content.ends_with('\n') && !new_content.ends_with('\n') {
            new_content.push('\n');
        }

        Some((new_content, matches_to_apply.len()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_exact_match() {
        let r = SimpleReplacer;
        let result = r.try_replace("hello world", "world", "rust", false);
        assert_eq!(result, Some(("hello rust".to_string(), 1)));
    }

    #[test]
    fn test_simple_no_match() {
        let r = SimpleReplacer;
        assert!(r.try_replace("hello", "world", "rust", false).is_none());
    }

    #[test]
    fn test_line_trimmed_match() {
        let r = LineTrimmedReplacer;
        let content = "  hello\n  world\n";
        let result = r.try_replace(content, "hello\nworld", "foo\nbar", false);
        assert!(result.is_some());
        let (new_content, count) = result.unwrap();
        assert_eq!(count, 1);
        assert!(new_content.contains("foo"));
    }

    #[test]
    fn test_whitespace_normalized() {
        let r = WhitespaceNormalizedReplacer;
        let content = "hello   world\n";
        let result = r.try_replace(content, "hello world", "hello rust", false);
        assert!(result.is_some());
    }

    #[test]
    fn test_all_strategies_chain() {
        let strategies = all_strategies();
        assert_eq!(strategies.len(), 3);

        // Test that the chain finds an exact match
        let content = "hello world";
        for strategy in &strategies {
            if let Some((result, count)) = strategy.try_replace(content, "world", "rust", false) {
                assert_eq!(result, "hello rust");
                assert_eq!(count, 1);
                assert_eq!(strategy.name(), "simple"); // Should be found by first strategy
                return;
            }
        }
        panic!("No strategy matched");
    }
}
