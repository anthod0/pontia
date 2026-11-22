use std::borrow::Cow;

const QUOTE_CHARACTER_LIMIT: usize = 2000;
const OPEN: &str = "<user_quote ";
const CLOSE: &str = "</user_quote>";

/// The quoted Bot chat message ID.
pub(crate) struct Target(pub String);

/// Normalize new user input only; stored messages and retries keep their original content.
pub(crate) fn user_input(content: &str) -> (Cow<'_, str>, Option<Target>) {
    match parse_quote(content) {
        Some((normalized, id)) => (normalized, Some(id)),
        None => (Cow::Borrowed(content), None),
    }
}

fn parse_quote(content: &str) -> Option<(Cow<'_, str>, Target)> {
    let (attributes, rest) = content.strip_prefix(OPEN)?.split_once('>')?;
    let (target, excerpt, file) = quote_attributes(attributes)?;
    let (text, after) = rest.split_once(CLOSE)?;
    if text.is_empty() || after.strip_prefix('\n')?.trim().is_empty() {
        return None;
    }

    // Count XML text characters, keeping entity references intact in the stored quote.
    let mut remaining = text;
    let mut end = None;
    let mut count = 0;
    while !remaining.is_empty() {
        if count == QUOTE_CHARACTER_LIMIT {
            end = Some(text.len() - remaining.len());
        }
        remaining = next_character(remaining)?.1;
        count += 1;
    }
    let Some(end) = end.filter(|_| !file) else {
        return Some((Cow::Borrowed(content), target));
    };
    let flag = if excerpt { "" } else { " excerpt=\"true\"" };
    Some((
        Cow::Owned(format!(
            "{OPEN}{attributes}{flag}>{}…{CLOSE}{after}",
            &text[..end]
        )),
        target,
    ))
}

/// Returns the target, whether the quote is an excerpt, and whether it quotes a file of the message.
fn quote_attributes(mut attributes: &str) -> Option<(Target, bool, bool)> {
    let mut target = None;
    let mut speaker = None;
    let mut file_path = false;
    let mut excerpt = false;
    while !attributes.trim().is_empty() {
        let (name, rest) = attributes.trim_start().split_once('=')?;
        let rest = rest.trim_start();
        let delimiter = rest.chars().next()?;
        if !matches!(delimiter, '\'' | '"') {
            return None;
        }
        let (value, rest) = rest[1..].split_once(delimiter)?;
        let mut encoded = value;
        let mut value = String::new();
        while !encoded.is_empty() {
            let (character, rest) = next_character(encoded)?;
            value.push(character);
            encoded = rest;
        }
        match name.trim_end() {
            "message_id" if target.is_none() => {
                let id = value.strip_prefix("bmsg_")?;
                let uuid = uuid::Uuid::parse_str(id).ok()?;
                if uuid.get_version_num() != 7
                    || uuid.get_variant() != uuid::Variant::RFC4122
                    || uuid.to_string() != id
                {
                    return None;
                }
                target = Some(Target(value));
            }
            "speaker" if speaker.is_none() && matches!(value.as_str(), "User" | "Bot") => {
                speaker = Some(value)
            }
            "path" if !file_path && value.starts_with('/') => file_path = true,
            "excerpt" if !excerpt && value == "true" => excerpt = true,
            _ => return None,
        }
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        attributes = rest;
    }
    let (target, speaker) = (target?, speaker?);
    if file_path && (speaker != "Bot" || excerpt) {
        return None;
    }
    Some((target, excerpt, file_path))
}

fn next_character(text: &str) -> Option<(char, &str)> {
    match text.chars().next()? {
        '<' => None,
        '&' => {
            let (entity, rest) = text[1..].split_once(';')?;
            let character = match entity {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let number = if let Some(hex) = entity.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16).ok()?
                    } else {
                        entity.strip_prefix('#')?.parse().ok()?
                    };
                    char::from_u32(number)?
                }
            };
            Some((character, rest))
        }
        character => Some((character, &text[character.len_utf8()..])),
    }
}
