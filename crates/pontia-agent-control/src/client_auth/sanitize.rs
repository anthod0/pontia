//! Cleaning of client output before it reaches logs, API responses, or SSE.

const REDACTED: &str = "[REDACTED]";
const MAX_LEN: usize = 512;
const SENSITIVE_PARAMETERS: &[&str] = &[
    "code",
    "state",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "session_state",
    "key",
];
const SECRET_PREFIXES: &[&str] = &["sk-", "xai-", "gho_", "ghp_", "AIza"];

/// Strip terminal escapes, redact credentials, and truncate to 512 bytes.
pub fn sanitize_output(text: &str) -> String {
    let mut clean = redact(strip_ansi(text).trim());
    if clean.len() > MAX_LEN {
        let mut end = MAX_LEN;
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        clean.truncate(end);
    }
    clean
}

/// Remove CSI, OSC, and two-byte escape sequences and other control characters
/// except newlines and tabs.
pub fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '\u{1b}' {
            if !char.is_control() || matches!(char, '\n' | '\t') {
                output.push(char);
            }
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    output
}

fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || matches!(char, '_' | '-' | '.')
}

fn ends_value(char: char) -> bool {
    char.is_whitespace() || matches!(char, '&' | '"' | '\'' | '<' | '>' | '#' | ',' | ';' | ')')
}

fn redact(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    let mut bearer = false;
    while let Some(char) = rest.chars().next() {
        if !is_word(char) {
            output.push(char);
            rest = &rest[char.len_utf8()..];
            continue;
        }
        let end = rest.find(|char| !is_word(char)).unwrap_or(rest.len());
        let word = &rest[..end];
        let token = word.trim_end_matches('.');
        rest = &rest[end..];
        if bearer && !token.is_empty() {
            bearer = false;
            output.push_str(REDACTED);
            output.push_str(&word[token.len()..]);
            continue;
        }
        if word.eq_ignore_ascii_case("bearer") && rest.starts_with(' ') {
            bearer = true;
            output.push_str(word);
            continue;
        }
        bearer = false;
        if rest.starts_with('=')
            && SENSITIVE_PARAMETERS
                .iter()
                .any(|name| word.eq_ignore_ascii_case(name))
        {
            let value = rest[1..].find(ends_value).map_or(rest.len(), |end| end + 1);
            output.push_str(word);
            output.push('=');
            if value > 1 {
                output.push_str(REDACTED);
            }
            rest = &rest[value..];
            continue;
        }
        if is_secret(token) {
            output.push_str(REDACTED);
            output.push_str(&word[token.len()..]);
        } else {
            output.push_str(word);
        }
    }
    output
}

fn is_secret(token: &str) -> bool {
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 8)
    {
        return true;
    }
    let mut parts = token.split('.');
    token.starts_with("eyJ")
        && parts.clone().count() == 3
        && parts.all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_terminal_escapes() {
        assert_eq!(
            sanitize_output("\u{1b}[31mfailed\u{1b}[0m \u{1b}]0;title\u{7}now\r"),
            "failed now"
        );
        assert_eq!(strip_ansi("\u{1b}]8;;https://x\u{1b}\\link"), "link");
    }

    #[test]
    fn redacts_credentials() {
        let cases = [
            (
                "open http://127.0.0.1:1234/cb?code=abc123&state=xyz&other=1",
                "open http://127.0.0.1:1234/cb?code=[REDACTED]&state=[REDACTED]&other=1",
            ),
            (
                "access_token=aaa refresh_token=bbb id_token=ccc key=ddd",
                "access_token=[REDACTED] refresh_token=[REDACTED] id_token=[REDACTED] key=[REDACTED]",
            ),
            (
                "Authorization: Bearer abc.def-ghi",
                "Authorization: Bearer [REDACTED]",
            ),
            (
                "key sk-proj1234567890 and xai-abcdefghij.",
                "key [REDACTED] and [REDACTED].",
            ),
            (
                "tokens ghp_abcdefgh12 gho_abcdefgh12",
                "tokens [REDACTED] [REDACTED]",
            ),
            ("AIzaSyA1234567890", "[REDACTED]"),
            (
                "jwt eyJhbGciOi.eyJzdWIiOi.c2lnbmF0dXJl end",
                "jwt [REDACTED] end",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(sanitize_output(input), expected, "{input}");
        }
    }

    #[test]
    fn keeps_ordinary_text() {
        let text = "task-runner sk-short codes=1 encode=2 Login failed: device code expired";
        assert_eq!(sanitize_output(text), text);
    }

    #[test]
    fn truncates_on_a_character_boundary() {
        let clean = sanitize_output(&"é".repeat(400));
        assert!(clean.len() <= 512);
        assert_eq!(clean.chars().count(), 256);
    }
}
