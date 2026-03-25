/// Simple glob pattern matching for ignore patterns.
/// Supports: * (any chars), ? (single char), ** (recursive dirs).
#[derive(Debug, Clone)]
pub struct GlobPattern {
    pattern: String,
    parts: Vec<GlobPart>,
}

#[derive(Debug, Clone)]
enum GlobPart {
    Literal(String),
    Star,        // * — matches anything except /
    DoubleStar,  // ** — matches anything including /
    Question,    // ? — matches single char
}

impl GlobPattern {
    pub fn new(pattern: &str) -> Self {
        let parts = Self::parse(pattern);
        Self {
            pattern: pattern.to_string(),
            parts,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    fn parse(pattern: &str) -> Vec<GlobPart> {
        let mut parts = Vec::new();
        let mut chars = pattern.chars().peekable();
        let mut literal = String::new();

        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if !literal.is_empty() {
                        parts.push(GlobPart::Literal(std::mem::take(&mut literal)));
                    }
                    if chars.peek() == Some(&'*') {
                        chars.next(); // consume second *
                        // Skip trailing / after **
                        if chars.peek() == Some(&'/') {
                            chars.next();
                        }
                        parts.push(GlobPart::DoubleStar);
                    } else {
                        parts.push(GlobPart::Star);
                    }
                }
                '?' => {
                    if !literal.is_empty() {
                        parts.push(GlobPart::Literal(std::mem::take(&mut literal)));
                    }
                    parts.push(GlobPart::Question);
                }
                _ => {
                    literal.push(c);
                }
            }
        }

        if !literal.is_empty() {
            parts.push(GlobPart::Literal(literal));
        }

        parts
    }

    /// Check if a path matches this glob pattern.
    pub fn matches(&self, path: &str) -> bool {
        Self::match_parts(&self.parts, path)
    }

    /// Check if a filename (last component) matches this pattern.
    /// Useful for patterns like "*.pyc" that should match any file with that extension.
    pub fn matches_filename(&self, path: &str) -> bool {
        // If pattern contains no path separator, match against filename only
        if !self.pattern.contains('/')
            && let Some(filename) = path.rsplit('/').next() {
                return Self::match_parts(&self.parts, filename);
            }
        self.matches(path)
    }

    fn match_parts(parts: &[GlobPart], text: &str) -> bool {
        if parts.is_empty() {
            return text.is_empty();
        }

        match &parts[0] {
            GlobPart::Literal(lit) => {
                if let Some(rest) = text.strip_prefix(lit.as_str()) {
                    Self::match_parts(&parts[1..], rest)
                } else {
                    false
                }
            }
            GlobPart::Question => {
                if text.is_empty() {
                    return false;
                }
                let mut chars = text.chars();
                let c = chars.next().unwrap();
                if c == '/' {
                    return false;
                }
                Self::match_parts(&parts[1..], chars.as_str())
            }
            GlobPart::Star => {
                // * matches zero or more non-/ characters
                let remaining = &parts[1..];
                // Try matching zero chars, then one, two, etc.
                for (i, c) in text.char_indices() {
                    if c == '/' {
                        // Star doesn't cross directory boundaries
                        return Self::match_parts(remaining, &text[i..]);
                    }
                    if Self::match_parts(remaining, &text[i..]) {
                        return true;
                    }
                }
                // Try matching entire remaining text
                Self::match_parts(remaining, "")
            }
            GlobPart::DoubleStar => {
                // ** matches zero or more path components
                let remaining = &parts[1..];
                // Try matching at every position
                for (i, _) in text.char_indices() {
                    if Self::match_parts(remaining, &text[i..]) {
                        return true;
                    }
                }
                Self::match_parts(remaining, "")
            }
        }
    }
}

/// Check if a path should be ignored based on a list of glob patterns.
pub fn should_ignore(path: &str, patterns: &[GlobPattern]) -> bool {
    let normalized = path.replace('\\', "/");
    patterns.iter().any(|p| {
        p.matches_filename(&normalized)
            || normalized
                .split('/')
                .any(|component| p.matches(component))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_match() {
        let p = GlobPattern::new("hello");
        assert!(p.matches("hello"));
        assert!(!p.matches("world"));
        assert!(!p.matches("hello!"));
    }

    #[test]
    fn star_match_extension() {
        let p = GlobPattern::new("*.pyc");
        assert!(p.matches("test.pyc"));
        assert!(p.matches("foo.pyc"));
        assert!(!p.matches("test.py"));
        assert!(!p.matches("dir/test.pyc")); // star doesn't cross /
    }

    #[test]
    fn star_match_prefix() {
        let p = GlobPattern::new("test_*");
        assert!(p.matches("test_foo"));
        assert!(p.matches("test_bar_baz"));
        assert!(!p.matches("foo_test"));
    }

    #[test]
    fn double_star_match() {
        let p = GlobPattern::new("**/*.pyc");
        assert!(p.matches("dir/test.pyc"));
        assert!(p.matches("a/b/c/test.pyc"));
        assert!(p.matches("test.pyc"));
        assert!(!p.matches("test.py"));
    }

    #[test]
    fn question_mark() {
        let p = GlobPattern::new("test?.py");
        assert!(p.matches("test1.py"));
        assert!(p.matches("testA.py"));
        assert!(!p.matches("test.py"));
        assert!(!p.matches("test12.py"));
    }

    #[test]
    fn matches_filename_simple() {
        let p = GlobPattern::new("*.pyc");
        assert!(p.matches_filename("src/test.pyc"));
        assert!(p.matches_filename("a/b/c.pyc"));
        assert!(!p.matches_filename("src/test.py"));
    }

    #[test]
    fn matches_filename_with_path() {
        let p = GlobPattern::new("src/*.rs");
        assert!(p.matches_filename("src/main.rs"));
        assert!(!p.matches_filename("test/main.rs"));
    }

    #[test]
    fn should_ignore_matching() {
        let patterns = vec![
            GlobPattern::new("*.pyc"),
            GlobPattern::new("__pycache__"),
            GlobPattern::new(".git"),
            GlobPattern::new("node_modules"),
        ];

        assert!(should_ignore("test.pyc", &patterns));
        assert!(should_ignore("src/test.pyc", &patterns));
        assert!(should_ignore("__pycache__/cache.py", &patterns));
        assert!(should_ignore(".git/config", &patterns));
        assert!(should_ignore("node_modules/express/index.js", &patterns));
        assert!(!should_ignore("src/main.rs", &patterns));
        assert!(!should_ignore("tests/test_auth.py", &patterns));
    }

    #[test]
    fn should_ignore_target_dir() {
        let patterns = vec![GlobPattern::new("target")];
        assert!(should_ignore("target/debug/testx", &patterns));
        assert!(!should_ignore("src/target_utils.rs", &patterns));
    }

    #[test]
    fn empty_pattern() {
        let p = GlobPattern::new("");
        assert!(p.matches(""));
        assert!(!p.matches("something"));
    }

    #[test]
    fn star_only() {
        let p = GlobPattern::new("*");
        assert!(p.matches("anything"));
        assert!(p.matches(""));
        assert!(!p.matches("dir/file")); // * doesn't cross /
    }

    #[test]
    fn double_star_only() {
        let p = GlobPattern::new("**");
        assert!(p.matches("anything"));
        assert!(p.matches("dir/file"));
        assert!(p.matches("a/b/c/d"));
        assert!(p.matches(""));
    }

    #[test]
    fn pattern_accessor() {
        let p = GlobPattern::new("*.rs");
        assert_eq!(p.pattern(), "*.rs");
    }

    #[test]
    fn complex_pattern() {
        let p = GlobPattern::new("src/**/test_*.rs");
        assert!(p.matches("src/test_foo.rs"));
        assert!(p.matches("src/adapters/test_foo.rs"));
        assert!(!p.matches("tests/test_foo.rs"));
    }

    #[test]
    fn backslash_normalization() {
        let patterns = vec![GlobPattern::new(".git")];
        assert!(should_ignore(".git\\config", &patterns));
    }
}
