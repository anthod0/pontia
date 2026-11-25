//! Files a message refers to, read from its text alone.

use serde::Serialize;

use super::MessageKind;

const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentSource {
    /// A file the user uploaded with the message.
    UserUpload,
    /// A path in an `attachments` fenced code block of a Bot reply.
    AttachmentBlock,
    /// A Markdown link or image to an absolute local path in a Bot reply.
    MarkdownLink,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    File,
    Image,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Attachment {
    pub source: AttachmentSource,
    pub media_kind: MediaKind,
    pub path: String,
    pub name: String,
}

/// The files of a message, each path once, with attachments block paths first.
pub(crate) fn attachments(kind: MessageKind, content: &str) -> Vec<Attachment> {
    match kind {
        MessageKind::UserInput => uploads(content),
        MessageKind::BotReply => reply_files(content),
    }
}

fn attachment(source: AttachmentSource, path: String, name: Option<String>) -> Attachment {
    let base = path.rsplit('/').next().unwrap_or_default();
    let image = base.rsplit_once('.').is_some_and(|(_, extension)| {
        IMAGE_EXTENSIONS
            .iter()
            .any(|image| extension.eq_ignore_ascii_case(image))
    });
    Attachment {
        source,
        media_kind: if image {
            MediaKind::Image
        } else {
            MediaKind::File
        },
        name: name.unwrap_or_else(|| base.to_owned()),
        path,
    }
}

/// The uploads block right after an optional leading quote; quotes and secrets are not files.
fn uploads(mut content: &str) -> Vec<Attachment> {
    if content.starts_with("<user_quote ")
        && let Some((_, body)) = content.split_once("</user_quote>\n")
    {
        content = body;
    }
    let Some(block) = content
        .strip_prefix("<user_uploads ")
        .and_then(|rest| rest.split_once("</user_uploads>"))
        .map(|(block, _)| block)
    else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for line in block.lines().skip(1) {
        let Some((path, name)) = line
            .trim()
            .strip_prefix("<user_upload path=\"")
            .and_then(|tag| tag.strip_suffix("</user_upload>"))
            .and_then(|tag| tag.split_once("\">"))
        else {
            continue;
        };
        let path = unescape_xml(path);
        if path.starts_with('/') && !files.iter().any(|file: &Attachment| file.path == path) {
            files.push(attachment(
                AttachmentSource::UserUpload,
                path,
                Some(unescape_xml(name)),
            ));
        }
    }
    files
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#9;", "\t")
        .replace("&#10;", "\n")
        .replace("&#13;", "\r")
        .replace("&amp;", "&")
}

struct Fence {
    marker: char,
    length: usize,
    attachments: bool,
    paths: Vec<String>,
    valid: bool,
}

/// Paths of closed top-level `attachments` blocks, then local links outside code.
fn reply_files(content: &str) -> Vec<Attachment> {
    let mut blocks: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut fence: Option<Fence> = None;
    for line in content.lines() {
        let marker = fence_marker(line);
        if let Some(open) = &mut fence {
            if marker.is_some_and(|(character, length, info)| {
                character == open.marker && length >= open.length && info.is_empty()
            }) {
                if open.attachments && open.valid {
                    blocks.append(&mut open.paths);
                }
                fence = None;
            } else {
                let line = line.trim();
                if line.starts_with('/') {
                    open.paths.push(line.to_owned());
                } else if !line.is_empty() {
                    open.valid = false;
                }
            }
        } else if let Some((character, length, info)) =
            marker.filter(|&(character, _, info)| character == '~' || !info.contains('`'))
        {
            fence = Some(Fence {
                marker: character,
                length,
                attachments: character == '`'
                    && info.split_whitespace().next() == Some("attachments"),
                paths: Vec::new(),
                valid: true,
            });
        } else {
            local_links(&without_code_spans(line), &mut links);
        }
    }
    let mut files: Vec<Attachment> = Vec::new();
    for (source, path) in blocks
        .into_iter()
        .map(|path| (AttachmentSource::AttachmentBlock, path))
        .chain(
            links
                .into_iter()
                .map(|path| (AttachmentSource::MarkdownLink, path)),
        )
    {
        if !files.iter().any(|file| file.path == path) {
            files.push(attachment(source, path, None));
        }
    }
    files
}

/// The fence character, run length and info string of a line that opens or closes a fence.
fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    let character = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = rest.len() - rest.trim_start_matches(character).len();
    (length >= 3).then(|| (character, length, rest[length..].trim()))
}

/// The line with inline code spans blanked out.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let run = after.len() - after.trim_start_matches('`').len();
        let body = &after[run..];
        match closing_run(body, run) {
            Some(end) => {
                out.push(' ');
                rest = &body[end + run..];
            }
            None => {
                out.push_str(&after[..run]);
                rest = body;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The offset of the next backtick run of exactly `run` characters.
fn closing_run(text: &str, run: usize) -> Option<usize> {
    let mut offset = 0;
    while let Some(found) = text[offset..].find('`') {
        let start = offset + found;
        let length = text[start..].len() - text[start..].trim_start_matches('`').len();
        if length == run {
            return Some(start);
        }
        offset = start + length;
    }
    None
}

/// Absolute local paths of `[text](/path)` and `![alt](/path)` links, without a fragment or line suffix.
fn local_links(line: &str, paths: &mut Vec<String>) {
    let mut offset = 0;
    while let Some(found) = line[offset..].find("](") {
        let start = offset + found;
        offset = start + 2;
        if !line[..start].contains('[') {
            continue;
        }
        let rest = &line[offset..];
        let destination = if let Some(inner) = rest.strip_prefix('<') {
            match inner.find('>') {
                Some(end) => &inner[..end],
                None => continue,
            }
        } else {
            let mut depth = 0usize;
            let end = rest
                .char_indices()
                .find(|&(_, character)| match character {
                    '(' => {
                        depth += 1;
                        false
                    }
                    ')' if depth == 0 => true,
                    ')' => {
                        depth -= 1;
                        false
                    }
                    character => character.is_whitespace(),
                })
                .map_or(rest.len(), |(index, _)| index);
            &rest[..end]
        };
        if let Some(path) = local_path(destination)
            && !paths.contains(&path)
        {
            paths.push(path);
        }
    }
}

fn local_path(destination: &str) -> Option<String> {
    let target = destination.split('#').next().unwrap_or_default();
    if !target.starts_with('/') || target.starts_with("//") || target.contains('?') {
        return None;
    }
    let path = percent_decode(strip_line_suffix(target))?;
    (!path.ends_with('/')).then_some(path)
}

/// Removes a `:line` or `:line:column` suffix.
fn strip_line_suffix(path: &str) -> &str {
    let mut path = path;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((head, digits))
                if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                path = head;
            }
            _ => break,
        }
    }
    path
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        AttachmentSource::{self, *},
        MediaKind::{self, *},
        *,
    };

    fn files(
        kind: MessageKind,
        content: &str,
    ) -> Vec<(AttachmentSource, MediaKind, String, String)> {
        attachments(kind, content)
            .into_iter()
            .map(|file| (file.source, file.media_kind, file.path, file.name))
            .collect()
    }

    fn expected<const N: usize>(
        files: [(AttachmentSource, MediaKind, &str, &str); N],
    ) -> Vec<(AttachmentSource, MediaKind, String, String)> {
        files
            .into_iter()
            .map(|(source, kind, path, name)| (source, kind, path.to_owned(), name.to_owned()))
            .collect()
    }

    const UPLOADS: &str =
        "<user_uploads note=\"Temporary files; copy any you need into the workspace.\">";
    const SECRETS: &str = "<user_secrets note=\"Secret files.\">";

    #[test]
    fn reads_uploads_after_a_quote_and_never_secrets() {
        let content = format!(
            "<user_quote message_id=\"bmsg_x\" speaker=\"Bot\">see &lt;user_upload path=\"/q.png\"&gt;</user_quote>\n\
             {UPLOADS}\n\
             <user_upload path=\"/up/a&amp;b.PNG\">a&amp;b.PNG</user_upload>\n\
             <user_upload path=\"/up/notes.txt\">notes.txt</user_upload>\n\
             </user_uploads>\n\
             {SECRETS}\n\
             <user_secret path=\"/secret/key.png\">ab****</user_secret>\n\
             </user_secrets>\n\
             look at [this](/linked.png)"
        );
        assert_eq!(
            files(MessageKind::UserInput, &content),
            expected([
                (UserUpload, Image, "/up/a&b.PNG", "a&b.PNG"),
                (UserUpload, File, "/up/notes.txt", "notes.txt"),
            ])
        );
        let secrets_only =
            format!("{SECRETS}\n<user_secret path=\"/s.png\">x</user_secret>\n</user_secrets>\nhi");
        assert!(files(MessageKind::UserInput, &secrets_only).is_empty());
        let late =
            format!("hello\n{UPLOADS}\n<user_upload path=\"/a\">a</user_upload>\n</user_uploads>");
        assert!(files(MessageKind::UserInput, &late).is_empty());
    }

    #[test]
    fn reads_closed_attachment_blocks_and_local_links_outside_code() {
        let content = "Done ![shot](/out/shot.webp) and [report](/out/report.md#L3) \
             [same](/out/a.png) [web](https://x.dev/a.png) [rel](docs/b.md) [proto](//cdn/c.png)\n\
             `[inline](/inline.md)` and [line](/src/main.rs:12:4) [space](</out/my file.txt>) [enc](/out/%E4%B8%AD.gif)\n\
             ```attachments\n/out/a.png\n\n/out/b.JPEG\n```\n\
             ```text\n[code](/in/code.md)\n```\n\
             ```attachments\n/out/c.png\nnot a path\n```\n\
             ~~~\n[tilde](/in/tilde.md)\n~~~\n\
             ```attachments\n/out/unclosed.png\n";
        assert_eq!(
            files(MessageKind::BotReply, content),
            expected([
                (AttachmentBlock, Image, "/out/a.png", "a.png"),
                (AttachmentBlock, Image, "/out/b.JPEG", "b.JPEG"),
                (MarkdownLink, Image, "/out/shot.webp", "shot.webp"),
                (MarkdownLink, File, "/out/report.md", "report.md"),
                (MarkdownLink, File, "/src/main.rs", "main.rs"),
                (MarkdownLink, File, "/out/my file.txt", "my file.txt"),
                (MarkdownLink, Image, "/out/中.gif", "中.gif"),
            ])
        );
    }

    #[test]
    fn treats_only_known_image_extensions_as_images() {
        let content = "```attachments\n/a.gif\n/b.jpg\n/c.svg\n/d.png.txt\n/png\n```";
        let kinds: Vec<_> = files(MessageKind::BotReply, content)
            .into_iter()
            .map(|(_, kind, _, _)| kind)
            .collect();
        assert_eq!(kinds, [Image, Image, File, File, File]);
    }
}
