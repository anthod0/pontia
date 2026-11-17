use std::{collections::HashSet, fmt::Write};

use super::{Error, Result};

pub(super) struct Keywords {
    pub words: Vec<String>,
    tokens: Vec<String>,
}

impl Keywords {
    pub fn parse(input: Vec<String>) -> Result<Self> {
        if input.is_empty() || input.len() > 32 {
            return Err(Error::InvalidInput(
                "keywords must contain 1–32 strings".into(),
            ));
        }
        let mut words = Vec::new();
        let mut tokens = Vec::new();
        let mut seen = HashSet::new();
        for word in input {
            if word.chars().any(|c| c.is_control() && !c.is_whitespace()) {
                return Err(Error::InvalidInput(
                    "keywords cannot contain control characters".into(),
                ));
            }
            let word = word
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if word.is_empty() || word.chars().count() > 128 {
                return Err(Error::InvalidInput(
                    "each keyword must contain 1–128 characters".into(),
                ));
            }
            if seen.insert(word.clone()) {
                // One ASCII token per agent-supplied keyword, including Chinese short words,
                // spaces and punctuation. User text can never become FTS query syntax.
                let mut token = String::from("k");
                for byte in word.as_bytes() {
                    write!(token, "{byte:02x}").expect("writing to String");
                }
                words.push(word);
                tokens.push(token);
            }
        }
        Ok(Self { words, tokens })
    }

    pub fn index_text(&self) -> String {
        self.tokens.join(" ")
    }

    pub fn query(&self) -> String {
        self.tokens.join(" OR ")
    }
}
