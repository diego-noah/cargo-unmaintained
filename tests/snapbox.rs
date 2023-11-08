#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::Result;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use serde::Deserialize;
use snapbox::{
    assert_matches_path,
    cmd::{cargo_bin, Command},
};
use std::{
    env::var,
    ffi::OsStr,
    fs::{read_dir, read_to_string},
};
use tempfile::tempdir;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Test {
    /// Repo url
    url: String,

    /// Repo revision; `None` (the default) means the head of the default branch
    #[serde(default)]
    rev: Option<String>,
}

#[test]
fn snapbox() -> Result<()> {
    let mut read_dir = read_dir("tests/cases")?;

    let test_paths = read_dir.try_fold(Vec::new(), |mut url_paths, entry| {
        let entry = entry?;
        let path = entry.path();
        if path.extension() == Some(OsStr::new("toml")) {
            url_paths.push(path);
        }
        Result::<_>::Ok(url_paths)
    })?;

    test_paths
        .into_par_iter()
        .panic_fuse()
        .try_for_each(|input_path| {
            let stderr_path = input_path.with_extension(format!("{}.stderr", token_modifier()));
            let stdout_path = input_path.with_extension(format!("{}.stdout", token_modifier()));

            let raw = read_to_string(input_path)?;

            let test: Test = toml::from_str(&raw).unwrap();

            let tempdir = tempdir()?;

            let mut command =
                Command::new("git").args(["clone", &test.url, &tempdir.path().to_string_lossy()]);
            if test.rev.is_none() {
                command = command.arg("--depth=1");
            }
            command.assert().success();

            if let Some(rev) = &test.rev {
                Command::new("git")
                    .args(["checkout", rev])
                    .current_dir(&tempdir)
                    .assert()
                    .success();
            }

            let output = Command::new(cargo_bin("cargo-unmaintained"))
                .args(["unmaintained", "--color=never", "--imprecise"])
                .current_dir(&tempdir)
                .output()?;

            let stderr_actual = String::from_utf8(output.stderr)?;
            let stdout_actual = String::from_utf8(output.stdout)?;

            // smoelius: Compare stderr before stdout so that you can see any errors that occurred.
            assert_matches_path(stderr_path, stderr_actual);
            assert_matches_path(stdout_path, stdout_actual);

            Ok(())
        })
}

fn token_modifier() -> &'static str {
    if var("GITHUB_TOKEN_PATH").is_ok() {
        "with_token"
    } else {
        "without_token"
    }
}

static RES: Lazy<[Regex; 2]> = Lazy::new(|| {
    [
        Regex::new(r"([^ ]*) days").unwrap(),
        Regex::new(r"latest: ([^ )]*)").unwrap(),
    ]
});

#[test]
fn snapbox_expected() -> Result<()> {
    for entry in read_dir("tests/cases")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("stdout")) {
            continue;
        }
        let contents = read_to_string(path)?;
        for line in contents.lines() {
            for re in &*RES {
                if let Some(captures) = re.captures(line) {
                    assert_eq!(2, captures.len());
                    assert_eq!("[..]", &captures[1]);
                }
            }
        }
    }

    Ok(())
}
