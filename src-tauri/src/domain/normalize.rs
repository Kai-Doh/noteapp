/// Single shared normalization function for `nodes.title_normalized` and
/// `aliases.normalized_alias` — link/alias resolution only works if both sides of
/// the lookup went through the exact same normalization.
pub fn normalize_title(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for c in title.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("untitled");
    }
    slug
}
