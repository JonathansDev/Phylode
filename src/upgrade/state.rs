//! What `install` and the upgrade helper remember between runs, in a folder only root can
//! change: which version is installed and where it came from, the copy kept for putting the
//! previous version back, versions that must not be tried again, and the history the weekly
//! summary and `phylode status` read.
//!
//! What breaks if you change it: an older version reads what a newer one wrote, because a
//! version that is put back reads the history its successor recorded. Add record kinds
//! freely (unknown ones are skipped), but never change what an existing line means.

use std::fmt;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use super::Version;
use crate::journal::{self, Civil, LocalZone};

/// The folder itself: root's, readable by everyone so `phylode status` and the weekly summary
/// need no sudo, writable only by root because the helper runs what it finds here as root.
pub const STATE_DIR: &str = "/usr/local/libexec/phylode";
/// `<version>\t<source>`, written by every successful install.
pub const INSTALLED_PATH: &str = "/usr/local/libexec/phylode/installed";
/// One record per line; see [`Record`].
pub const HISTORY_PATH: &str = "/usr/local/libexec/phylode/history.tsv";
/// Versions never to try again, one per line.
pub const REJECTED_PATH: &str = "/usr/local/libexec/phylode/rejected";
/// `<version>\t<YYYY-MM-DD>`: the version the helper last ran an install of, and the day.
pub const TRIED_PATH: &str = "/usr/local/libexec/phylode/tried";
/// Where the helper copies Homebrew's download before checking it.
pub const INCOMING_PATH: &str = "/usr/local/libexec/phylode/incoming";
/// The files `install` replaced, kept until the next install.
pub const PREVIOUS_DIR: &str = "/usr/local/libexec/phylode/previous";
/// Held by the helper while it runs, so only one runs at a time.
pub const LOCK_PATH: &str = "/usr/local/libexec/phylode/lock";

/// Where the installed copy came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Installed from what Homebrew downloaded; `brew upgrade` updates it.
    Homebrew,
    /// Built from the source code; updated by the README's steps.
    Built,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Self::Homebrew => "homebrew",
            Self::Built => "source",
        }
    }

    fn from_wire(text: &str) -> Option<Self> {
        match text {
            "homebrew" => Some(Self::Homebrew),
            "source" => Some(Self::Built),
            _ => None,
        }
    }

    /// How `phylode status` names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "installed from Homebrew",
            Self::Built => "built from source",
        }
    }
}

/// The version `install` last put in place and where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Installed {
    version: Version,
    source: Source,
}

impl Installed {
    pub fn new(version: Version, source: Source) -> Self {
        Self { version, source }
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// What the last install wrote; `None` before the first, or after an install by a version
    /// that did not write it.
    pub fn read() -> Option<Self> {
        Self::parse(&fs::read_to_string(INSTALLED_PATH).ok()?)
    }

    fn parse(text: &str) -> Option<Self> {
        let (version, source) = text.trim_end().split_once('\t')?;
        Some(Self {
            version: Version::parse(version)?,
            source: Source::from_wire(source)?,
        })
    }

    pub fn wire(&self) -> String {
        format!("{}\t{}\n", self.version, self.source.as_str())
    }
}

/// Why a new version was put back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// It did not answer within the first 15 seconds.
    NotAnswering,
    /// It answered, then stopped within 60 seconds.
    Stopped,
}

impl Fault {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotAnswering => "not-answering",
            Self::Stopped => "stopped",
        }
    }

    fn from_wire(text: &str) -> Option<Self> {
        match text {
            "not-answering" => Some(Self::NotAnswering),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAnswering => "did not answer within 15 seconds",
            Self::Stopped => "stopped within 60 seconds of starting",
        })
    }
}

/// Why the helper would not use what Homebrew downloaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// It differs from the checksum published with that version on GitHub.
    Checksum,
    /// It is a link, a device or something else that is not an ordinary file, or too large.
    NotAFile,
}

impl Refusal {
    fn as_str(self) -> &'static str {
        match self {
            Self::Checksum => "checksum",
            Self::NotAFile => "not-a-file",
        }
    }

    fn from_wire(text: &str) -> Option<Self> {
        match text {
            "checksum" => Some(Self::Checksum),
            "not-a-file" => Some(Self::NotAFile),
            _ => None,
        }
    }
}

/// One thing that happened to the installed version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// `brew upgrade` brought a new version and it is in place. `from` is unknown when the
    /// version before did not record itself.
    Upgraded { from: Option<Version>, to: Version },
    /// A new version failed after being swapped in, and the one before is back.
    PutBack { tried: Version, fault: Fault },
    /// Homebrew's copy was not used.
    Refused { version: Version, refusal: Refusal },
    /// The checksum could not be fetched, so the new version waits for the next try.
    Waiting { version: Version },
    /// Installing the new version failed for another reason; it is tried again the next day.
    Failed { version: Version, detail: String },
}

impl Change {
    /// The sentence the weekly summary and `phylode status` show.
    pub fn sentence(&self) -> String {
        match self {
            Self::Upgraded {
                from: Some(from),
                to,
            } => format!("Upgraded from {from} to {to}"),
            Self::Upgraded { from: None, to } => format!("Upgraded to {to}"),
            Self::PutBack { tried, fault } => format!(
                "Phylode {tried} {fault}, so the version before it was put back; {tried} will not \
                 be tried again"
            ),
            Self::Refused {
                version,
                refusal: Refusal::Checksum,
            } => format!(
                "Did not install {version}: Homebrew's copy does not match the checksum published \
                 for it on GitHub; it will not be tried again"
            ),
            Self::Refused {
                version,
                refusal: Refusal::NotAFile,
            } => format!(
                "Did not install {version}: Homebrew's copy is not an ordinary program file; it \
                 will not be tried again"
            ),
            Self::Waiting { version } => format!(
                "Phylode {version} is waiting: GitHub could not be reached to check it, so it is \
                 tried again daily"
            ),
            Self::Failed { version, detail } => {
                format!("Could not install {version}, so it is tried again daily: {detail}")
            }
        }
    }

    fn wire(&self) -> (&'static str, Version, String) {
        match self {
            Self::Upgraded { from, to } => (
                "upgraded",
                *to,
                from.map_or_else(|| "-".to_string(), |v| v.to_string()),
            ),
            Self::PutBack { tried, fault } => ("put-back", *tried, fault.as_str().to_string()),
            Self::Refused { version, refusal } => {
                ("refused", *version, refusal.as_str().to_string())
            }
            Self::Waiting { version } => ("waiting", *version, "-".to_string()),
            Self::Failed { version, detail } => ("failed", *version, one_line(detail)),
        }
    }

    fn from_wire(kind: &str, version: Version, detail: &str) -> Option<Self> {
        Some(match kind {
            "upgraded" => Self::Upgraded {
                from: Version::parse(detail),
                to: version,
            },
            "put-back" => Self::PutBack {
                tried: version,
                fault: Fault::from_wire(detail)?,
            },
            "refused" => Self::Refused {
                version,
                refusal: Refusal::from_wire(detail)?,
            },
            "waiting" => Self::Waiting { version },
            "failed" => Self::Failed {
                version,
                detail: detail.to_string(),
            },
            _ => return None,
        })
    }
}

/// A line of the history: `YYYY-MM-DD HH:MM:SS\t<kind>\t<version>\t<detail>`, in the Mac's
/// time zone like the journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    timestamp: String,
    change: Change,
}

impl Record {
    fn new(timestamp: String, change: Change) -> Self {
        Self { timestamp, change }
    }

    /// `YYYY-MM-DD HH:MM:SS`.
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    /// `YYYY-MM-DD`, the day it falls on.
    pub fn day(&self) -> &str {
        self.timestamp.get(..10).unwrap_or(&self.timestamp)
    }

    pub fn change(&self) -> &Change {
        &self.change
    }

    /// A line that does not parse is skipped rather than failing the whole history.
    pub fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let timestamp = fields.next()?;
        let kind = fields.next()?;
        let version = Version::parse(fields.next()?)?;
        let detail = fields.next().unwrap_or("-");
        Some(Self {
            timestamp: timestamp.to_string(),
            change: Change::from_wire(kind, version, detail)?,
        })
    }

    fn wire(&self) -> String {
        let (kind, version, detail) = self.change.wire();
        format!(
            "{}\t{kind}\t{version}\t{detail}\n",
            one_line(&self.timestamp)
        )
    }
}

/// Every record that parses, oldest first.
pub fn parse_history(text: &str) -> Vec<Record> {
    text.lines().filter_map(Record::parse).collect()
}

/// The history as written; empty when there is none or it cannot be read.
pub fn history_text() -> String {
    fs::read_to_string(HISTORY_PATH).unwrap_or_default()
}

/// The most recent record, for `phylode status`.
pub fn last_record() -> Option<Record> {
    parse_history(&history_text()).pop()
}

/// Appends a record stamped now. A [`Change::Waiting`] or [`Change::Failed`] that repeats the
/// last record is not written again, so a week of the same trouble is one line, not seven.
pub fn record(change: Change) -> std::io::Result<()> {
    record_in(
        Path::new(HISTORY_PATH),
        change,
        journal::unix_now(),
        &local_zone(),
    )
}

fn record_in(path: &Path, change: Change, now: i64, zone: &LocalZone) -> std::io::Result<()> {
    if matches!(change, Change::Waiting { .. } | Change::Failed { .. }) {
        let previous = fs::read_to_string(path).unwrap_or_default();
        if parse_history(&previous)
            .last()
            .is_some_and(|last| last.change == change)
        {
            return Ok(());
        }
    }
    let record = Record::new(Civil::from_unix(now, zone).timestamp_string(), change);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o644)
        .open(path)?;
    file.write_all(record.wire().as_bytes())
}

/// Whether `version` was put back or refused before.
pub fn is_rejected(version: Version) -> bool {
    rejected_in(
        &fs::read_to_string(REJECTED_PATH).unwrap_or_default(),
        version,
    )
}

fn rejected_in(text: &str, version: Version) -> bool {
    text.lines()
        .any(|line| Version::parse(line) == Some(version))
}

/// Remembers `version` so it is never tried again.
pub fn reject(version: Version) -> std::io::Result<()> {
    if is_rejected(version) {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o644)
        .open(REJECTED_PATH)?;
    writeln!(file, "{version}")
}

/// Whether the helper already ran an install of `version` today. One whose install failed
/// stays newer than the installed copy, and any change in Homebrew's folder wakes the helper,
/// so without this it would be tried at every wake rather than once a day.
pub fn tried_today(version: Version) -> bool {
    tried_in(
        &fs::read_to_string(TRIED_PATH).unwrap_or_default(),
        version,
        &today(),
    )
}

fn tried_in(text: &str, version: Version, today: &str) -> bool {
    let mut fields = text.trim_end().split('\t');
    fields.next().and_then(Version::parse) == Some(version)
        && fields.next() == Some(today)
        && fields.next().is_none()
}

/// Notes that the helper is about to run an install of `version` today.
pub fn mark_tried(version: Version) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(TRIED_PATH)?;
    writeln!(file, "{version}\t{}", today())
}

/// `YYYY-MM-DD` in the Mac's time zone, as the history's days are.
fn today() -> String {
    let stamp = Civil::from_unix(journal::unix_now(), &local_zone()).timestamp_string();
    stamp.get(..10).unwrap_or(&stamp).to_string()
}

fn local_zone() -> LocalZone {
    LocalZone::system().unwrap_or_else(|_| LocalZone::utc())
}

/// Tabs and line breaks would split a record.
fn one_line(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("phylode-state-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Should change: each kind of change survives being written and read back.
    #[test]
    fn every_change_reads_back_as_written() {
        let changes = [
            Change::Upgraded {
                from: Some(v("1.2.0")),
                to: v("1.3.0"),
            },
            Change::Upgraded {
                from: None,
                to: v("1.2.0"),
            },
            Change::PutBack {
                tried: v("1.3.0"),
                fault: Fault::NotAnswering,
            },
            Change::PutBack {
                tried: v("1.3.0"),
                fault: Fault::Stopped,
            },
            Change::Refused {
                version: v("1.3.0"),
                refusal: Refusal::Checksum,
            },
            Change::Refused {
                version: v("1.3.0"),
                refusal: Refusal::NotAFile,
            },
            Change::Waiting {
                version: v("1.3.0"),
            },
            Change::Failed {
                version: v("1.3.0"),
                detail: "a\tb\nc".to_string(),
            },
        ];
        for change in changes {
            let record = Record::new("2026-11-25 09:30:00".to_string(), change.clone());
            let back = Record::parse(record.wire().trim_end()).unwrap();
            let expected = match change {
                Change::Failed { version, .. } => Change::Failed {
                    version,
                    detail: "a b c".to_string(),
                },
                other => other,
            };
            assert_eq!(back.change(), &expected);
            assert_eq!(back.day(), "2026-11-25");
        }
    }

    /// Should change: lines a newer version may write, and damaged lines, are skipped.
    /// Should not change: the lines around them are still read.
    #[test]
    fn unknown_and_damaged_lines_are_skipped() {
        let text = "2026-11-24 10:00:00\tupgraded\t1.2.0\t1.1.3\n\
                    2026-11-24 11:00:00\tsomething-new\t1.3.0\tx\n\
                    garbage\n\
                    2026-11-24 12:00:00\trefused\t1.3.0-beta\tchecksum\n\
                    2026-11-25 10:00:00\twaiting\t1.3.0\t-\n";
        let records = parse_history(text);
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].change(),
            &Change::Upgraded {
                from: Some(v("1.1.3")),
                to: v("1.2.0")
            }
        );
        assert_eq!(
            records[1].change(),
            &Change::Waiting {
                version: v("1.3.0")
            }
        );
    }

    /// Should change: a second identical waiting record is not written.
    /// Should not change: waiting for another version, or after something else, is written.
    #[test]
    fn a_repeated_wait_is_written_once() {
        let dir = scratch("wait");
        let path = dir.join("history.tsv");
        let zone = LocalZone::utc();
        let wait = |version: &str| Change::Waiting {
            version: v(version),
        };
        record_in(&path, wait("1.3.0"), 0, &zone).unwrap();
        record_in(&path, wait("1.3.0"), 86_400, &zone).unwrap();
        record_in(&path, wait("1.4.0"), 2 * 86_400, &zone).unwrap();
        let upgraded = Change::Upgraded {
            from: Some(v("1.2.0")),
            to: v("1.4.0"),
        };
        record_in(&path, upgraded, 3 * 86_400, &zone).unwrap();
        record_in(&path, wait("1.4.0"), 4 * 86_400, &zone).unwrap();
        let records = parse_history(&fs::read_to_string(&path).unwrap());
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].timestamp(), "1970-01-01 00:00:00");
        assert_eq!(records[1].day(), "1970-01-03");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Should change: a second identical failure is not written.
    /// Should not change: a failure with another cause, or after something else, is written.
    #[test]
    fn a_repeated_failure_is_written_once() {
        let dir = scratch("failed");
        let path = dir.join("history.tsv");
        let zone = LocalZone::utc();
        let failed = |detail: &str| Change::Failed {
            version: v("1.3.0"),
            detail: detail.to_string(),
        };
        record_in(&path, failed("disk full"), 0, &zone).unwrap();
        record_in(&path, failed("disk full"), 86_400, &zone).unwrap();
        record_in(&path, failed("port 53 is taken"), 2 * 86_400, &zone).unwrap();
        let wait = Change::Waiting {
            version: v("1.3.0"),
        };
        record_in(&path, wait, 3 * 86_400, &zone).unwrap();
        record_in(&path, failed("port 53 is taken"), 4 * 86_400, &zone).unwrap();
        let records = parse_history(&fs::read_to_string(&path).unwrap());
        assert_eq!(records.len(), 4);
        assert_eq!(records[1].day(), "1970-01-03");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Should change: only the same version on the same day counts as tried today.
    #[test]
    fn a_version_counts_as_tried_only_on_its_day() {
        let text = "1.3.0\t2026-11-25\n";
        assert!(tried_in(text, v("1.3.0"), "2026-11-25"));
        assert!(!tried_in(text, v("1.3.0"), "2026-11-26"));
        assert!(!tried_in(text, v("1.3.1"), "2026-11-25"));
        assert!(!tried_in("", v("1.3.0"), "2026-11-25"));
        assert!(!tried_in(
            "1.3.0\t2026-11-25\textra\n",
            v("1.3.0"),
            "2026-11-25"
        ));
    }

    #[test]
    fn the_installed_record_reads_back() {
        let installed = Installed::new(v("1.2.0"), Source::Homebrew);
        assert_eq!(Installed::parse(&installed.wire()), Some(installed));
        let built = Installed::new(v("1.2.0"), Source::Built);
        assert_eq!(Installed::parse(&built.wire()), Some(built));
        assert_eq!(Installed::parse("1.2.0\tsomewhere\n"), None);
        assert_eq!(Installed::parse("1.2\thomebrew\n"), None);
    }

    #[test]
    fn a_rejected_version_is_found_only_exactly() {
        let text = "1.3.0\n1.4.1\n";
        assert!(rejected_in(text, v("1.3.0")));
        assert!(rejected_in(text, v("1.4.1")));
        assert!(!rejected_in(text, v("1.4.0")));
        assert!(!rejected_in("", v("1.3.0")));
    }
}
