//! The upgrade helper, `phylode run upgrade`: after `brew upgrade` downloads a new version, it
//! copies that version somewhere only root can change, checks it against the checksum
//! published for it on GitHub, and only then runs it as a full `install`, which puts the
//! version before back if the new one does not keep answering.
//!
//! launchd runs it as root when Homebrew changes its `opt` folder and once a day. It only
//! exists on a Mac installed from Homebrew.
//!
//! What breaks if you change it: Homebrew's folder can be changed without a password, so
//! nothing read from it may decide anything before the checksum matched. The version comes
//! from a folder name and is checked against a strict pattern before it is used in a path or
//! an address; the program is copied, never run, until its copy matches; and the copy refuses
//! links, devices and oversized files, which could otherwise hang or exhaust a process running
//! as root.

use std::fmt;
use std::fs;
use std::io::{self, Read as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use thiserror::Error;

use crate::build::CHECKSUM_URL_PREFIX;

pub mod state;

use state::{Change, Refusal, Source};

/// Homebrew's folder on Apple silicon, which the account that installed Homebrew can change
/// without a password. On Intel it is `/usr/local`, which holds the installed copy too, so
/// Homebrew installs are for Apple silicon only.
pub const HOMEBREW_PREFIX: &str = "/opt/homebrew";
/// Homebrew's link to the installed version of the formula, `../Cellar/phylode/<version>`.
const HOMEBREW_OPT: &str = "/opt/homebrew/opt/phylode";
const HOMEBREW_CELLAR: &str = "/opt/homebrew/Cellar/phylode";

/// A program larger than this is not Phylode; the release build is about 10 MB.
const MAX_PROGRAM_BYTES: u64 = 64 * 1024 * 1024;
/// A checksum file is one line; anything longer is not one.
const MAX_CHECKSUM_BYTES: u64 = 4096;
const CURL: &str = "/usr/bin/curl";

/// Values from macOS's `<sys/fcntl.h>`; the tests below prove each does what its name says.
/// Refuse a link anywhere in the path.
const O_NOFOLLOW_ANY: i32 = 0x2000_0000;
/// Do not wait on a pipe or device that has no writer.
const O_NONBLOCK: i32 = 0x0000_0004;

/// A release number, `major.minor.patch`, digits only. Anything else (a pre-release, a
/// Homebrew revision suffix, a path) is not a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version([u32; 3]);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = [0u32; 3];
        let mut fields = text.split('.');
        for part in &mut parts {
            let field = fields.next()?;
            let digits_only = !field.is_empty()
                && field.len() <= 9
                && field.bytes().all(|b| b.is_ascii_digit())
                && (field == "0" || !field.starts_with('0'));
            if !digits_only {
                return None;
            }
            *part = field.parse().ok()?;
        }
        fields.next().is_none().then_some(Self(parts))
    }

    /// This program's own version.
    pub fn current() -> Option<Self> {
        Self::parse(env!("CARGO_PKG_VERSION"))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
    }
}

#[derive(Debug, Error)]
pub enum UpgradeError {
    #[error("the upgrade helper needs administrator rights; launchd runs it as root")]
    NotRoot,
    #[error("this program's version {0} is not a plain release number")]
    OwnVersion(&'static str),
    #[error("could not access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not run the new version's install: {0}")]
    Spawn(#[source] io::Error),
}

/// What one run of the helper did, for its log.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Another run holds the lock.
    Busy,
    /// The installed copy was not installed from Homebrew.
    NotFromHomebrew,
    /// Homebrew has no Phylode, or not a newer one.
    NothingNewer,
    /// That version was put back or refused before.
    AlreadyRejected(Version),
    /// An install of that version already ran today and did not finish; it is tried again
    /// the next day.
    TriedToday(Version),
    /// Homebrew has not finished putting the new version in place.
    NotReady(Version),
    /// The checksum could not be fetched; the next run tries again.
    Waiting(Version),
    Refused(Version, Refusal),
    /// The new version's install ran and exited with this code (it records its own result).
    Installed(Version, Option<i32>),
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => write!(f, "another upgrade check is running"),
            Self::NotFromHomebrew => write!(f, "Phylode was not installed from Homebrew"),
            Self::NothingNewer => write!(f, "Homebrew has no newer version"),
            Self::AlreadyRejected(v) => write!(f, "{v} was put back or refused before"),
            Self::TriedToday(v) => write!(f, "{v} could not be installed today; trying tomorrow"),
            Self::NotReady(v) => write!(f, "Homebrew has not finished installing {v}"),
            Self::Waiting(v) => write!(f, "could not fetch the checksum for {v}"),
            Self::Refused(v, Refusal::Checksum) => write!(f, "{v} does not match its checksum"),
            Self::Refused(v, Refusal::NotAFile) => write!(f, "{v} is not an ordinary file"),
            Self::Installed(v, Some(0)) => write!(f, "installed {v}"),
            Self::Installed(v, code) => write!(f, "installing {v} failed (exit {code:?})"),
        }
    }
}

/// One run of the helper. Never runs anything from Homebrew's folder: what it runs is the
/// copy in [`state::INCOMING_PATH`], and only once that copy matches its checksum.
pub fn run() -> Result<Step, UpgradeError> {
    if !crate::install::running_as_root() {
        return Err(UpgradeError::NotRoot);
    }
    let installed =
        Version::current().ok_or(UpgradeError::OwnVersion(env!("CARGO_PKG_VERSION")))?;
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(state::LOCK_PATH)
        .map_err(|source| io_error(state::LOCK_PATH, source))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Ok(Step::Busy),
        Err(fs::TryLockError::Error(source)) => return Err(io_error(state::LOCK_PATH, source)),
    }

    if state::Installed::read().map(|i| i.source()) != Some(Source::Homebrew) {
        return Ok(Step::NotFromHomebrew);
    }
    let Some(offered) = homebrew_version(Path::new(HOMEBREW_OPT)) else {
        return Ok(Step::NothingNewer);
    };
    if offered <= installed {
        return Ok(Step::NothingNewer);
    }
    if state::is_rejected(offered) {
        return Ok(Step::AlreadyRejected(offered));
    }
    if state::tried_today(offered) {
        return Ok(Step::TriedToday(offered));
    }

    let incoming = Path::new(state::INCOMING_PATH);
    match copy_program(&cellar_program(offered), incoming) {
        Ok(()) => {}
        Err(CopyError::Missing) => return Ok(Step::NotReady(offered)),
        Err(CopyError::NotAFile) => {
            refuse(offered, Refusal::NotAFile);
            return Ok(Step::Refused(offered, Refusal::NotAFile));
        }
        Err(CopyError::Io(path, source)) => return Err(UpgradeError::Io { path, source }),
    }
    let Some(expected) = fetch_checksum(offered) else {
        let _ = fs::remove_file(incoming);
        record(Change::Waiting { version: offered });
        return Ok(Step::Waiting(offered));
    };
    let actual = sha256_file(incoming).map_err(|source| io_error(state::INCOMING_PATH, source))?;
    if actual != expected {
        let _ = fs::remove_file(incoming);
        refuse(offered, Refusal::Checksum);
        return Ok(Step::Refused(offered, Refusal::Checksum));
    }

    if let Err(source) = state::mark_tried(offered) {
        let _ = fs::remove_file(incoming);
        return Err(io_error(state::TRIED_PATH, source));
    }
    // Its own process group: if the install reloads this helper's launchd job, launchd stops
    // this process, and the install must carry on to the end.
    let status = Command::new(incoming)
        .arg("install")
        .stdin(Stdio::null())
        .process_group(0)
        .status()
        .map_err(UpgradeError::Spawn)?;
    let _ = fs::remove_file(incoming);
    Ok(Step::Installed(offered, status.code()))
}

/// The version Homebrew has linked, read from the name of the folder its `opt` link points
/// at. Never asks the program itself: that would run it before it is checked.
fn homebrew_version(opt_link: &Path) -> Option<Version> {
    let target = fs::read_link(opt_link).ok()?;
    Version::parse(target.file_name()?.to_str()?)
}

fn cellar_program(version: Version) -> PathBuf {
    Path::new(HOMEBREW_CELLAR)
        .join(version.to_string())
        .join("bin/phylode")
}

#[derive(Debug)]
enum CopyError {
    /// Nothing there yet.
    Missing,
    /// A link, a pipe, a device or a file too large to be the program.
    NotAFile,
    Io(PathBuf, io::Error),
}

/// Copies the ordinary file at `from` to `to`, root's and executable, by writing beside it
/// and renaming over it.
fn copy_program(from: &Path, to: &Path) -> Result<(), CopyError> {
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW_ANY | O_NONBLOCK)
        .open(from)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(CopyError::Missing),
        // ELOOP: a link somewhere in the path.
        Err(error) if error.raw_os_error() == Some(62) => return Err(CopyError::NotAFile),
        Err(error) => return Err(CopyError::Io(from.to_path_buf(), error)),
    };
    let meta = file
        .metadata()
        .map_err(|error| CopyError::Io(from.to_path_buf(), error))?;
    if !meta.is_file() || meta.len() > MAX_PROGRAM_BYTES {
        return Err(CopyError::NotAFile);
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROGRAM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CopyError::Io(from.to_path_buf(), error))?;
    if bytes.len() as u64 > MAX_PROGRAM_BYTES {
        return Err(CopyError::NotAFile);
    }
    let staged = to.with_extension("phylode-new");
    let write = || -> io::Result<()> {
        let _ = fs::remove_file(&staged);
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&staged)?;
        io::Write::write_all(&mut out, &bytes)?;
        out.sync_all()?;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        fs::rename(&staged, to)
    };
    write().map_err(|error| CopyError::Io(to.to_path_buf(), error))
}

/// The SHA-256 published for `version`'s program, or `None` when it could not be fetched or
/// is not a checksum. Fetched with the system's curl, HTTPS only, from the release download
/// address rather than GitHub's API, which limits how often it may be asked.
fn fetch_checksum(version: Version) -> Option<[u8; 32]> {
    let url = format!("{CHECKSUM_URL_PREFIX}/{version}/phylode-{version}-arm64.bin.sha256");
    let protocols = if CHECKSUM_URL_PREFIX.starts_with("https://") {
        "=https"
    } else {
        "=https,file"
    };
    let mut child = Command::new(CURL)
        .args(["-fsSL", "--proto", protocols, "--proto-redir", "=https"])
        .args(["--tlsv1.2", "--max-time", "60", "--max-filesize"])
        .arg(MAX_CHECKSUM_BYTES.to_string())
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut text = Vec::new();
    let read = child
        .stdout
        .take()
        .map(|out| out.take(MAX_CHECKSUM_BYTES + 1).read_to_end(&mut text));
    if text.len() as u64 > MAX_CHECKSUM_BYTES {
        let _ = child.kill();
    }
    let finished = child.wait().ok()?;
    if !finished.success() || !matches!(read, Some(Ok(_))) {
        return None;
    }
    parse_checksum(std::str::from_utf8(&text).ok()?)
}

/// The first word of a `shasum -a 256` line, as 32 bytes.
fn parse_checksum(text: &str) -> Option<[u8; 32]> {
    let hex = text.split_whitespace().next()?;
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (byte, pair) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let bytes = fs::read(path)?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    Ok(out)
}

fn refuse(version: Version, refusal: Refusal) {
    let _ = state::reject(version);
    record(Change::Refused { version, refusal });
}

/// A history that cannot be written is said in the helper's log, and the work goes on.
fn record(change: Change) {
    if let Err(error) = state::record(change) {
        crate::log_line!("Warning: could not write {}: {error}", state::HISTORY_PATH);
    }
}

fn io_error(path: &str, source: io::Error) -> UpgradeError {
    UpgradeError::Io {
        path: PathBuf::from(path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt as _;

    use super::*;

    /// Resolved, because the temporary folder is reached through `/var`, a link to
    /// `/private/var`, which [`copy_program`] would refuse.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("phylode-upgrade-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    /// Should change: anything but three plain numbers is not a version.
    /// Should not change: plain release numbers parse and order numerically.
    #[test]
    fn only_plain_release_numbers_are_versions() {
        for good in ["1.2.0", "0.0.1", "10.20.30", "999999999.0.0"] {
            assert_eq!(Version::parse(good).unwrap().to_string(), good);
        }
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1.2.3_1",
            "1.2.3-beta",
            "01.2.3",
            "1.2.x",
            "../1.2.3",
            "1.2. 3",
            "+1.2.3",
            "1.2.3/",
            "1234567890.0.0",
            "１.2.3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert!(Version::parse("1.10.0") > Version::parse("1.9.9"));
        assert!(Version::parse("2.0.0") > Version::parse("1.99.99"));
    }

    #[test]
    fn this_programs_version_is_a_plain_release_number() {
        assert!(
            Version::current().is_some(),
            "{}",
            env!("CARGO_PKG_VERSION")
        );
    }

    /// Should change: the version comes from the folder the link points at, and a name that is
    /// not a version gives none.
    #[test]
    fn homebrews_version_is_read_from_the_link_not_the_program() {
        let dir = scratch("opt");
        let link = dir.join("phylode");
        std::os::unix::fs::symlink("../Cellar/phylode/1.4.2", &link).unwrap();
        assert_eq!(homebrew_version(&link), Version::parse("1.4.2"));
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../Cellar/phylode/1.4.2_1", &link).unwrap();
        assert_eq!(homebrew_version(&link), None);
        fs::remove_file(&link).unwrap();
        assert_eq!(homebrew_version(&link), None);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Should change: a link anywhere in the path, a pipe and an oversized file are refused,
    /// and opening the pipe does not wait for a writer.
    /// Should not change: an ordinary file is copied whole, executable, nothing left beside it.
    #[test]
    fn only_an_ordinary_file_is_copied() {
        let dir = scratch("copy");
        let out = dir.join("out").join("incoming");
        fs::create_dir_all(out.parent().unwrap()).unwrap();

        let real = dir.join("real");
        fs::create_dir_all(real.join("bin")).unwrap();
        fs::write(real.join("bin/phylode"), b"program").unwrap();
        copy_program(&real.join("bin/phylode"), &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"program");
        assert_eq!(fs::metadata(&out).unwrap().mode() & 0o777, 0o755);
        assert_eq!(fs::read_dir(out.parent().unwrap()).unwrap().count(), 1);

        let linked_folder = dir.join("linked");
        std::os::unix::fs::symlink(&real, &linked_folder).unwrap();
        assert!(matches!(
            copy_program(&linked_folder.join("bin/phylode"), &out),
            Err(CopyError::NotAFile)
        ));
        let linked_file = dir.join("linked-file");
        std::os::unix::fs::symlink(real.join("bin/phylode"), &linked_file).unwrap();
        assert!(matches!(
            copy_program(&linked_file, &out),
            Err(CopyError::NotAFile)
        ));

        let fifo = dir.join("fifo");
        assert!(
            Command::new("/usr/bin/mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let started = std::time::Instant::now();
        assert!(matches!(
            copy_program(&fifo, &out),
            Err(CopyError::NotAFile)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));

        let big = dir.join("big");
        fs::File::create(&big)
            .unwrap()
            .set_len(MAX_PROGRAM_BYTES + 1)
            .unwrap();
        assert!(matches!(copy_program(&big, &out), Err(CopyError::NotAFile)));

        assert!(matches!(
            copy_program(&dir.join("absent"), &out),
            Err(CopyError::Missing)
        ));
        assert_eq!(
            fs::read(&out).unwrap(),
            b"program",
            "a refusal leaves the last copy"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Expected values from `printf 'abc' | shasum -a 256` (also FIPS 180-2's first example).
    #[test]
    fn the_checksum_is_sha256_of_the_copy() {
        let dir = scratch("sha");
        let path = dir.join("abc");
        fs::write(&path, b"abc").unwrap();
        let expected = parse_checksum(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  abc\n",
        )
        .unwrap();
        assert_eq!(sha256_file(&path).unwrap(), expected);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_checksum_line_must_hold_64_hex_digits() {
        let line = format!("{}  phylode\n", "0f".repeat(32));
        assert_eq!(parse_checksum(&line), Some([0x0f; 32]));
        assert_eq!(parse_checksum(&"0f".repeat(31)), None);
        assert_eq!(parse_checksum(&format!("{}zz", "0f".repeat(31))), None);
        assert_eq!(parse_checksum("<html>Not Found</html>"), None);
        assert_eq!(parse_checksum(""), None);
    }

    /// Release builds fetch from GitHub; only a test build may point anywhere else.
    #[test]
    fn checksums_come_from_github_releases() {
        if !crate::build::is_test_build() {
            assert_eq!(CHECKSUM_URL_PREFIX, crate::build::RELEASES);
            assert!(CHECKSUM_URL_PREFIX.starts_with("https://github.com/"));
        }
    }
}
