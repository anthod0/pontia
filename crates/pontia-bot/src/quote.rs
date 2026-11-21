use std::borrow::Cow;

const QUOTE_CHARACTER_LIMIT: usize = 2000;
const OPEN: &str = "<user_quote ";
const CLOSE: &str = "</user_quote>";

/// Normalize new user input only; stored messages and retries keep their original content.
pub(crate) fn limit_quote(content: &str) -> Cow<'_, str> {
    truncate_quote(content).map_or(Cow::Borrowed(content), Cow::Owned)
}

fn truncate_quote(content: &str) -> Option<String> {
    let (attributes, rest) = content.strip_prefix(OPEN)?.split_once('>')?;
    let excerpt = message_attributes(attributes)?;
    let (text, after) = rest.split_once(CLOSE)?;
    if after.strip_prefix('\n')?.trim().is_empty() {
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
    let end = end?;
    let flag = if excerpt { "" } else { " excerpt=\"true\"" };
    Some(format!(
        "{OPEN}{attributes}{flag}>{}…{CLOSE}{after}",
        &text[..end]
    ))
}

// Match the Dashboard's text-only message quote format; file quotes remain metadata.
fn message_attributes(mut attributes: &str) -> Option<bool> {
    let mut message = false;
    let mut speaker = false;
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
            "message_id" if !message => {
                let id = value.strip_prefix("bmsg_")?;
                let uuid = uuid::Uuid::parse_str(id).ok()?;
                if uuid.get_version_num() != 7
                    || uuid.get_variant() != uuid::Variant::RFC4122
                    || uuid.to_string() != id
                {
                    return None;
                }
                message = true;
            }
            "speaker" if !speaker && matches!(value.as_str(), "User" | "Bot") => speaker = true,
            "excerpt" if !excerpt && value == "true" => excerpt = true,
            _ => return None,
        }
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        attributes = rest;
    }
    (message && speaker).then_some(excerpt)
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
