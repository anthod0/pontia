/// Returns the product release version from the repository's canonical VERSION file.
pub fn version() -> &'static str {
    include_str!("../../../VERSION").trim()
}
