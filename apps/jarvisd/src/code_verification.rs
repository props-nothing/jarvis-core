//! Conservative delivery diagnostics, separate from a process's exit status.

use std::path::Path;

use serde_json::{Value, json};

/// Known build-tool messages which explicitly say a requested check did not run.
/// Ordinary warnings and occurrences of the word "error" are not failures.
pub(crate) fn reported_check_failures(stdout: &[u8], stderr: &[u8]) -> Vec<&'static str> {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
    .to_ascii_lowercase();
    [
        (
            "eslint must be installed in order to run during builds",
            "eslint_unavailable",
        ),
        ("skipping linting", "lint_skipped"),
        ("skipping validation of types", "typecheck_skipped"),
    ]
    .into_iter()
    .filter_map(|(message, code)| text.contains(message).then_some(code))
    .collect()
}

pub(crate) fn package(directory: &Path) -> Option<Value> {
    use std::io::Read as _;
    let file = std::fs::File::open(directory.join("package.json")).ok()?;
    let mut text = String::new();
    file.take(1_048_577).read_to_string(&mut text).ok()?;
    if text.len() > 1_048_576 {
        return None;
    }
    serde_json::from_str(&text).ok()
}

/// Catch advertised direct Node checks that cannot possibly run. Complex shell
/// scripts are left to their actual runner, not interpreted as shell code here.
pub(crate) fn missing_check_scripts(directory: &Path) -> Vec<Value> {
    let Some(package) = package(directory) else {
        return Vec::new();
    };
    package["scripts"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| {
            matches!(name.as_str(), "build" | "lint" | "typecheck" | "test")
                || name.starts_with("verify:")
        })
        .filter_map(|(name, script)| {
            let words: Vec<_> = script.as_str()?.split_whitespace().collect();
            if words.len() != 2 || words[0] != "node" || words[1].starts_with('-') {
                return None;
            }
            let path = words[1].trim_matches(['\'', '"']);
            (!directory.join(path).is_file())
                .then(|| json!({"script":name,"path":path,"reason":"check_script_missing"}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_skips_are_failures_but_ordinary_warnings_are_not() {
        assert_eq!(
            reported_check_failures(
                b"Compiled successfully",
                b"ESLint must be installed in order to run during builds"
            ),
            ["eslint_unavailable"]
        );
        assert_eq!(
            reported_check_failures(b"Skipping linting\nSkipping validation of types", b""),
            ["lint_skipped", "typecheck_skipped"]
        );
        assert!(
            reported_check_failures(b"test error_handling ... ok", b"Warning: bundle size")
                .is_empty()
        );
    }
}
