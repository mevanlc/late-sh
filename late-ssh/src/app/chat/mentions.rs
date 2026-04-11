/// Returns `true` if `c` is a valid character within a mention username.
pub(crate) fn is_mention_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Returns `true` if `@` at byte offset `at` in `text` starts a valid mention
/// (i.e. it is at the beginning or preceded by a non-mention character).
pub(crate) fn valid_mention_start(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }

    text[..at]
        .chars()
        .next_back()
        .map(|c| !is_mention_char(c))
        .unwrap_or(true)
}

/// Extract unique usernames from `@mention`s in a message body.
/// Returns deduplicated, lowercased usernames (without the `@` prefix).
pub(crate) fn extract_mentions(body: &str) -> Vec<String> {
    let mut usernames = Vec::new();
    let mut idx = 0;

    while idx < body.len() {
        let Some(ch) = body[idx..].chars().next() else {
            break;
        };

        if ch == '@' && valid_mention_start(body, idx) {
            let mut end = idx + ch.len_utf8();
            let mut has_mention_chars = false;

            while end < body.len() {
                let Some(next) = body[end..].chars().next() else {
                    break;
                };
                if !is_mention_char(next) {
                    break;
                }
                has_mention_chars = true;
                end += next.len_utf8();
            }

            if has_mention_chars {
                let username = body[idx + 1..end].to_ascii_lowercase();
                if !usernames.contains(&username) {
                    usernames.push(username);
                }
                idx = end;
                continue;
            }
        }

        idx += ch.len_utf8();
    }

    usernames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_single_mention() {
        assert_eq!(extract_mentions("hey @alice"), vec!["alice"]);
    }

    #[test]
    fn extract_multiple_mentions() {
        let result = extract_mentions("hey @alice and @Bob");
        assert_eq!(result, vec!["alice", "bob"]);
    }

    #[test]
    fn extract_deduplicates() {
        let result = extract_mentions("@alice @Alice @ALICE");
        assert_eq!(result, vec!["alice"]);
    }

    #[test]
    fn extract_ignores_email() {
        assert!(extract_mentions("mail me at hi@example.com").is_empty());
    }

    #[test]
    fn extract_ignores_bare_at() {
        assert!(extract_mentions("just @ here").is_empty());
    }

    #[test]
    fn extract_stops_at_punctuation() {
        let result = extract_mentions("@alice, nice one");
        assert_eq!(result, vec!["alice"]);
    }

    #[test]
    fn extract_handles_mention_with_special_chars() {
        let result = extract_mentions("hi @night-owl_123");
        assert_eq!(result, vec!["night-owl_123"]);
    }
}
