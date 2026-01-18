use regex::Regex;
use snapbox::assert_data_eq;
use std::{
    env::{remove_var, set_current_dir, var},
    fs::{read_to_string, write},
    io::{Write, stderr},
    process::Command,
    sync::LazyLock,
};
use testing::{Tee, split_at_cut_lines, split_at_first_cut_line, tee};

const PATH_STDOUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/rustsec_advisories.stdout"
);

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..");
}

#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
#[test]
fn rustsec_advisories() {
    let mut command = Command::new("cargo");
    command
        .args(["run", "--example=rustsec_advisories"])
        .env("RUST_BACKTRACE", "0");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_expected = read_to_string(PATH_STDOUT).unwrap();
    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    if var("BLESS").is_ok() {
        write(PATH_STDOUT, stdout_actual).unwrap();
        update_readme(stdout_actual);

        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "`{PATH_STDOUT}` was overwritten and may need to be adjusted."
        )
        .unwrap();
    } else {
        assert_data_eq!(
            above_cut_line(stdout_actual),
            above_cut_line(&stdout_expected),
        );
    }
}

fn update_readme(stdout: &str) {
    let (_, middle, bottom) = split_at_cut_lines(stdout).unwrap();

    let readme = read_to_string("../README.md").unwrap();

    let updated_with_as_of = replace_section(
        &readme,
        "<!-- as-of start -->",
        "<!-- as-of end -->",
        &format!("\n\n{}\n\n", middle.trim()),
    );

    let readme_with_as_of_and_not_identified = replace_section(
        &updated_with_as_of,
        "<!-- not-identified start -->",
        "<!-- not-identified end -->",
        &format!("\n\n{}\n\n", bottom.trim()),
    );

    write("../README.md", readme_with_as_of_and_not_identified).unwrap();
}

fn replace_section(content: &str, start_marker: &str, end_marker: &str, insertion: &str) -> String {
    let start = content.find(start_marker).unwrap();
    let end = content.find(end_marker).unwrap();

    let before = &content[..start + start_marker.len()];
    let after = &content[end..];

    format!("{before}{insertion}{after}")
}

fn above_cut_line(s: &str) -> &str {
    split_at_first_cut_line(s).map_or(s, |(above, _)| above)
}

static CANDIDATE_VERSIONS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^candidate versions found which didn't match: [0-9]").unwrap());
static TMP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/tmp\b").unwrap());

#[test]
fn sanitary() {
    let contents = read_to_string(PATH_STDOUT).unwrap();

    for line in contents.lines() {
        assert!(
            !CANDIDATE_VERSIONS_RE.is_match(line),
            "{line:?} matches `CANDIDATE_VERSIONS_RE`"
        );
        assert!(!TMP_RE.is_match(line), "{line:?} matches `TMP_RE`");
    }
}
