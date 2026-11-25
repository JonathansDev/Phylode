//! Runs the program as people type it: what the help lists, what bare `phylode` prints, and
//! that output to a reader that has already gone ends quietly.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a helper that panics fails the test that called it, which is how a test should fail"
)]

use std::process::{Command, Output, Stdio};

fn phylode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phylode"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("could not run phylode")
}

/// Runs the program with its output going to a reader that has already gone, as
/// `phylode report | head -1` leaves it once `head` has its line.
///
/// The write fails with a broken pipe rather than a signal, because Rust ignores SIGPIPE;
/// `println!` turns that failure into a panic and the program ends with a crash message.
#[test]
fn a_closed_reader_ends_the_program_quietly() {
    let (reader, writer) = std::io::pipe().expect("could not create a pipe");
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_phylode"))
        .arg("help")
        .stdout(writer)
        .stderr(Stdio::piped())
        .output()
        .expect("could not run phylode");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stderr.is_empty(),
        "phylode help with its reader gone exited with {:?} and printed {stderr:?}",
        output.status.code()
    );
}

/// Should change: the help lists exactly the commands people type, `feedback` instead of
/// `diagnostics`, and leaves out the `run` entries launchd uses.
#[test]
fn the_help_lists_exactly_the_commands_people_type() {
    let help = String::from_utf8(phylode(&["help"]).stdout).unwrap();
    let mut commands: Vec<&str> = help
        .lines()
        .filter_map(|line| {
            let line = line
                .trim_start()
                .strip_prefix("sudo ")
                .unwrap_or(line.trim_start());
            line.strip_prefix("phylode ")?.split_whitespace().next()
        })
        .collect();
    commands.sort_unstable();
    let mut expected = [
        "install",
        "uninstall",
        "on",
        "off",
        "allow",
        "block",
        "status",
        "report",
        "forget",
        "feedback",
        "--version",
    ];
    expected.sort_unstable();
    assert_eq!(commands, expected, "{help}");
    assert!(!help.contains("phylode run"), "{help}");
    assert!(!help.contains("diagnostics"), "{help}");
}

/// Should change: bare `phylode` prints the help and succeeds, rather than starting the
/// service in the foreground.
#[test]
fn bare_phylode_prints_the_help() {
    let bare = phylode(&[]);
    let help = phylode(&["help"]);
    assert!(bare.status.success());
    assert!(!bare.stdout.is_empty());
    assert_eq!(bare.stdout, help.stdout);
}

/// Should change: `feedback` prints the report `diagnostics` printed, from the same
/// renderer, and `diagnostics` is no longer a command.
#[test]
fn feedback_prints_the_report_for_a_bug_report() {
    let dir = std::env::temp_dir().join(format!("phylode-feedback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("missing.conf");
    let out = phylode(&["feedback", "--config", config.to_str().unwrap()]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(out.status.success());
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(format!("Phylode {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(lines.next().unwrap().starts_with("macOS "));
    assert_eq!(lines.next(), Some("Service answering: no"));
    assert!(!phylode(&["diagnostics"]).status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
