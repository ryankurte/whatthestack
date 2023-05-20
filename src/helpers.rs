use regex::Regex;

// Regexes for matching rust function names / prefixes
lazy_static::lazy_static! {
    static ref PREFIX: Regex = Regex::new(r"^([a-z0-9_:]+)(.*)").unwrap();
    static ref NAMES: Regex = Regex::new(r"(?:[a-z0-9_]+::)+([A-Z][a-z0-9_A-Z]+)").unwrap();
}

/// Helper to compress function names
pub fn compress_name(n: &str) -> String {
    // Strip obvious prefixes
    let mut s = PREFIX.replace_all(n, "$2").to_string();
    if s.len() == 0 {
        return n.to_string();
    }

    // Shorten names
    s = NAMES.replace_all(&s, "$1").to_string();

    // Return compressed form
    s.to_string()
}
