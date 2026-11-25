//! Settings fixed when the program is compiled. Only the cloud tests set them, to stand in
//! for GitHub and to make a version that fails on purpose; a release build leaves both unset,
//! which `release.yml` checks, and `phylode --version` names any build that set one.

/// Where the upgrade helper fetches each version's checksum, followed by
/// `/<version>/phylode-<version>-arm64.bin.sha256`. The cloud tests point it at a folder on
/// the runner (a `file://` address) to publish a checksum of their own.
pub const CHECKSUM_URL_PREFIX: &str = match option_env!("PHYLODE_TEST_CHECKSUM_URL_PREFIX") {
    Some(prefix) => prefix,
    None => RELEASES,
};

/// The releases of the public repository.
pub const RELEASES: &str = "https://github.com/JonathansDev/Phylode/releases/download";

/// Seconds after which `phylode run service` exits on purpose, standing in for a new version
/// that is broken: `0` never answers, `30` answers and then stops.
pub const TEST_EXIT_AFTER: Option<&str> = option_env!("PHYLODE_TEST_EXIT_AFTER");

/// Whether either test setting was given when compiling.
pub fn is_test_build() -> bool {
    CHECKSUM_URL_PREFIX != RELEASES || TEST_EXIT_AFTER.is_some()
}
