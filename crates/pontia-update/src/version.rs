use semver::Version;

/// Compare release precedence, ignoring build metadata as required by SemVer.
pub(super) fn is_newer(current: &str, release: &str) -> Result<bool, String> {
    let current =
        Version::parse(current).map_err(|error| format!("invalid installed version: {error}"))?;
    let release = release
        .strip_prefix('v')
        .ok_or("release version must begin with v")?;
    let release =
        Version::parse(release).map_err(|error| format!("invalid release version: {error}"))?;
    Ok(release.cmp_precedence(&current).is_gt())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn only_upgrades_to_a_higher_release_precedence() {
        for (current, release, upgrade) in [
            ("0.4.0", "v0.4.1", true),
            ("0.9.0", "v0.10.0", true),
            ("0.4.0", "v0.4.0", false),
            ("0.5.0", "v0.4.9", false),
            ("1.0.0-rc.2", "v1.0.0-rc.10", true),
            ("1.0.0-rc.1", "v1.0.0", true),
            ("1.0.0", "v1.0.0-rc.1", false),
            ("1.0.0+build.1", "v1.0.0+build.2", false),
        ] {
            assert_eq!(
                is_newer(current, release).unwrap(),
                upgrade,
                "{current} -> {release}"
            );
        }
    }

    #[test]
    fn rejects_malformed_versions() {
        for (current, release) in [
            ("unknown", "v1.0.0"),
            ("1.0.0", "1.1.0"),
            ("1.0.0", "v1.01.0"),
            ("1.0.0", "v1.0"),
        ] {
            assert!(is_newer(current, release).is_err());
        }
    }
}
