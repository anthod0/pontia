pub(super) fn short_session_id(session_id: &str) -> String {
    let id_body = session_id.rsplit('_').next().unwrap_or(session_id);
    let mut chars: Vec<char> = id_body.chars().rev().take(8).collect();
    chars.reverse();
    chars.into_iter().collect()
}
