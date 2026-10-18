//! Strict parser for the unchanged JARVIS-RELEASE-1 signed byte format.

use crate::{Error, Result};

pub(crate) const PLATFORMS: [&str; 5] = [
    "linux-x86_64",
    "linux-aarch64",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
];
pub(crate) const MAX_BINARY: u64 = 268_435_456;

pub(crate) fn version(value: &str) -> bool {
    let fields: Vec<_> = value.split('.').collect();
    fields.len() == 3
        && fields.iter().all(|field| {
            !field.is_empty() && field.len() <= 10 && field.bytes().all(|b| b.is_ascii_digit())
        })
}

pub(crate) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn selector(tag: &str, package: &str) -> Result<()> {
    if !version(package)
        || !(tag.strip_prefix("ci-").is_some_and(|commit| {
            commit.len() == 40 && digest(&format!("{commit}{}", "0".repeat(24)))
        }) || tag == format!("v{package}"))
    {
        return Err(Error::Refused(
            "choose an exact ci-<40 lowercase hex> or vMAJOR.MINOR.PATCH tag and matching package version",
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct Archive {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

pub(crate) fn parse(text: &str, tag: &str, package: &str, platform: &str) -> Result<Archive> {
    selector(tag, package)?;
    let lines: Vec<_> = text.split_terminator('\n').collect();
    let invalid = || {
        Error::Refused(
            "release manifest does not match the selected repository, release, version and platforms",
        )
    };
    if text.len() > 8192
        || !text.ends_with('\n')
        || lines.len() != 10
        || lines[0] != "JARVIS-RELEASE-1"
        || lines[1] != "repository=props-nothing/jarvis-core"
        || lines[2] != format!("tag={tag}")
        || lines[4] != format!("version={package}")
    {
        return Err(invalid());
    }
    let commit = lines[3].strip_prefix("commit=").ok_or_else(invalid)?;
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || (tag.starts_with("ci-") && tag != format!("ci-{commit}"))
    {
        return Err(invalid());
    }
    let mut selected = None;
    for (label, line) in PLATFORMS.iter().zip(&lines[5..]) {
        let fields: Vec<_> = line.split('\t').collect();
        let extension = if label.starts_with("windows") {
            "zip"
        } else {
            "tar.gz"
        };
        if fields.len() != 4
            || fields[0] != *label
            || fields[1] != format!("jarvis-{package}-{label}.{extension}")
            || !digest(fields[3])
        {
            return Err(invalid());
        }
        let bytes = fields[2].parse::<u64>().map_err(|_| invalid())?;
        if bytes == 0 || bytes > MAX_BINARY || fields[2] != bytes.to_string() {
            return Err(invalid());
        }
        if *label == platform {
            selected = Some(Archive {
                name: fields[1].to_owned(),
                bytes,
                sha256: fields[3].to_owned(),
            });
        }
    }
    selected.ok_or_else(invalid)
}
