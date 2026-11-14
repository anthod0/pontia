//! Shared styling for interactive command output. Colors turn off automatically when the
//! stream is not a terminal or `NO_COLOR` is set.

use std::fmt::Display;

use dialoguer::{
    console::{StyledObject, style},
    theme::ColorfulTheme,
};

pub fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn heading<D: Display>(text: D) -> StyledObject<D> {
    style(text).bold()
}

pub fn success<D: Display>(text: D) -> String {
    format!("{} {text}", style("✓").green().bold())
}

pub fn warning<D: Display>(text: D) -> String {
    format!("{} {}", style("!").yellow().bold(), style(text).yellow())
}

pub fn dim<D: Display>(text: D) -> StyledObject<D> {
    style(text).dim()
}

pub fn link<D: Display>(text: D) -> StyledObject<D> {
    style(text).cyan().underlined()
}

pub fn command<D: Display>(text: D) -> StyledObject<D> {
    style(text).cyan()
}

/// A yes/no question in the same shape as the themed dialoguer prompts.
pub fn question(prompt: &str, hint: &str) -> String {
    format!(
        "{} {} {} ",
        style("?").yellow(),
        style(prompt).bold(),
        style(hint).dim()
    )
}

/// The first summary line names the action; later lines are details.
pub fn summary_lines(lines: &[String], skipped: bool) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| match (index, skipped) {
            (0, false) => format!("  • {line}"),
            (0, true) => format!("  {}", dim(format!("• {line}"))),
            _ => format!("      {}", dim(line)),
        })
        .collect()
}

pub fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
