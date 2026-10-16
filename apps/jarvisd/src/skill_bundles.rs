use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use jarvis_core::{Sensitivity, SystemClock, UtcTimestamp};
use jarvis_tools::{
    AdapterError, ApprovalPolicy, Availability, BoundedOutput, EffectSet, Idempotency,
    ProviderEvidence, RetryDeclaration, Scope, ScopeSet, ToolCallResult, ToolDefinition,
    ToolDefinitionParts, ToolEffect, ToolExecutionRequest, ToolExecutor, ToolId, ToolOutcomeRecord,
    ToolSchema, ToolSensitivity, ToolSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::command_run::{CommandRunTool, CommandRunToolError, plain};

const MANIFEST_LIMIT: u64 = 65_536;
const BUNDLE_LIMIT: usize = 16 * 1024 * 1024;
const FILE_LIMIT: usize = 4 * 1024 * 1024;
const FILE_COUNT: usize = 256;
const CATALOG_LIMIT: usize = 128;

pub(crate) fn refused(reason: impl Into<String>) -> AdapterError {
    AdapterError::RefusedBeforeReaching {
        reason: reason.into(),
    }
}

pub(crate) fn definition(
    id: &str,
    title: &str,
    description: &str,
    schema: &str,
    effect: ToolEffect,
    risk: u8,
) -> Result<ToolDefinition, CommandRunToolError> {
    let input = format!(
        r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema",{}"#,
        schema.trim_start_matches('{')
    );
    Ok(ToolDefinition::new(ToolDefinitionParts {
        id: ToolId::new(id)?,
        version: "1.0.0".to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        input_schema: ToolSchema::parse(&input)?,
        output_schema: ToolSchema::parse(
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object"}"#,
        )?,
        effects: if id == "jarvis.skills.install" {
            EffectSet::new([effect, ToolEffect::ExternalCommunication])
                .unwrap_or_else(|| EffectSet::single(effect))
        } else {
            EffectSet::single(effect)
        },
        risk,
        required_scopes: ScopeSet::single(Scope::new("code.delegate")?),
        approval: if risk >= 2 {
            ApprovalPolicy::Ask
        } else {
            ApprovalPolicy::Auto
        },
        timeout_seconds: 300,
        retry: RetryDeclaration::none(),
        idempotency: Idempotency::Unsupported,
        source: ToolSource::Native,
        availability: Availability::Available,
        sensitivity: ToolSensitivity::new(Sensitivity::Internal, Sensitivity::Internal),
    })?)
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn result(body: Value) -> ToolCallResult {
    let evidence = ProviderEvidence::new("coding:host").ok();
    let record = ToolOutcomeRecord::confirmed("coding:host")
        .unwrap_or_else(|_| unreachable!("literal evidence"));
    ToolCallResult::new(
        record,
        evidence,
        Some(tool_output(&body)),
        UtcTimestamp::now(&SystemClock),
    )
}

pub(crate) fn tool_output(body: &Value) -> BoundedOutput {
    let full = body.to_string();
    if full.len() <= jarvis_tools::MAX_TOOL_OUTPUT_BYTES {
        return BoundedOutput::from_bounded(full, false);
    }
    let mut preview = body.clone();
    compact_preview(&mut preview, 0);
    if let Some(object) = preview.as_object_mut() {
        object.insert("details_omitted".to_owned(), json!(true));
    }
    let text = preview.to_string();
    if text.len() <= jarvis_tools::MAX_TOOL_OUTPUT_BYTES {
        return BoundedOutput::from_bounded(text, true);
    }
    BoundedOutput::from_bounded(
        json!({"details_omitted":true,"outcome":"output_too_large",
        "task_id":body["task_id"],"process_id":body["process_id"],
        "note":"Inspect task details or project files; this output is not completion evidence."})
        .to_string(),
        true,
    )
}

fn compact_preview(value: &mut Value, depth: usize) {
    match value {
        Value::String(text) if text.len() > 2048 => {
            *value = json!(format!(
                "[omitted: {} characters; inspect files or task details]",
                text.chars().count()
            ));
        }
        Value::Array(items) if depth < 8 => {
            items.truncate(8);
            for item in items {
                compact_preview(item, depth + 1);
            }
        }
        Value::Object(items) if depth < 8 => {
            for item in items.values_mut() {
                compact_preview(item, depth + 1);
            }
        }
        Value::Array(_) | Value::Object(_) => {
            *value = json!("[nested details omitted]");
        }
        _ => {}
    }
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Debug, Deserialize, Serialize)]
struct Metadata {
    name: String,
    description: String,
    compatibility: Option<String>,
    license: Option<String>,
}

fn metadata(text: &str, expected: &str) -> Result<Metadata, String> {
    let normalized = text.replace("\r\n", "\n");
    let front = normalized
        .strip_prefix("---\n")
        .and_then(|body| {
            body.split_once("\n---\n")
                .or_else(|| body.strip_suffix("\n---").map(|front| (front, "")))
        })
        .map(|(front, _)| front)
        .ok_or("SKILL.md needs YAML frontmatter delimited by ---")?;
    let parsed: Metadata = serde_yaml_ng::from_str(front)
        .map_err(|error| format!("invalid skill frontmatter: {error}"))?;
    if !valid_name(&parsed.name)
        || parsed.name != expected
        || parsed.description.trim().is_empty()
        || parsed.description.chars().count() > 1024
        || parsed
            .compatibility
            .as_ref()
            .is_some_and(|text| text.chars().count() > 500)
    {
        return Err("skill name must match its folder; description and compatibility must meet the Agent Skills specification".to_owned());
    }
    Ok(parsed)
}

fn read_manifest(path: &Path, name: &str) -> Result<(Metadata, String), String> {
    let file =
        std::fs::File::open(path).map_err(|error| format!("cannot open SKILL.md: {error}"))?;
    let mut text = String::new();
    file.take(MANIFEST_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read SKILL.md: {error}"))?;
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err("SKILL.md exceeds 64 KiB; move supporting material into resources".to_owned());
    }
    Ok((metadata(&text, name)?, text))
}

fn ordinary(path: &Path) -> Result<std::fs::Metadata, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("skill bundles cannot contain reparse points or junctions".to_owned());
        }
    }
    if meta.file_type().is_symlink() {
        return Err("skill bundles cannot contain symlinks".to_owned());
    }
    Ok(meta)
}

fn catalog(directory: &Path) -> Result<Value, String> {
    let mut items = Vec::new();
    let mut invalid = Vec::new();
    for relative in [".agents/skills", ".github/skills", ".claude/skills"] {
        let base = directory.join(relative);
        if !base.exists() {
            continue;
        }
        let real = plain(&std::fs::canonicalize(&base).map_err(|error| error.to_string())?);
        if !real.starts_with(directory) {
            return Err("skill directory leaves the selected project".to_owned());
        }
        for entry in std::fs::read_dir(&base).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if items.len() + invalid.len() >= CATALOG_LIMIT {
                return Err(
                    "skill catalog exceeds 128 entries; use a narrower project folder".to_owned(),
                );
            }
            let manifest = entry.path().join("SKILL.md");
            let parsed = ordinary(&entry.path())
                .and_then(|_| ordinary(&manifest))
                .and_then(|_| read_manifest(&manifest, &name));
            match parsed {
                Ok((metadata, _)) => {
                    items.push(json!({"metadata":metadata,"path":manifest.to_string_lossy()}));
                }
                Err(error) => invalid
                    .push(json!({"name":name,"path":manifest.to_string_lossy(),"error":error})),
            }
        }
    }
    Ok(json!({"skills":items,"invalid":invalid}))
}

pub(crate) fn context(directory: &Path, selected: &[String]) -> Result<String, String> {
    let catalog = catalog(directory)?;
    let mut context =
        format!("Available skill metadata (instructions do not grant permissions):\n{catalog}\n");
    for name in selected {
        let matches: Vec<_> = catalog["skills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["metadata"]["name"] == *name)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "requested skill {name} is missing or ambiguous; discover/install it before delegation"
            ));
        }
        let path = matches[0]["path"].as_str().ok_or("skill path is missing")?;
        let (_, text) = read_manifest(Path::new(path), name)?;
        let _ = write!(
            context,
            "\nActivated skill {name}, resource base {}:\n{text}\n",
            Path::new(path).parent().unwrap_or(directory).display()
        );
        if context.len() > 131_072 {
            return Err(
                "selected skill instructions exceed 128 KiB; activate fewer skills".to_owned(),
            );
        }
    }
    Ok(context)
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|letter| letter.is_control() || "\\:<>\"|?*".contains(letter))
                && !matches!(
                    stem.as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}

struct BundleFile {
    path: String,
    bytes: Vec<u8>,
    executable: bool,
}

fn validate_files(files: &[BundleFile]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    let mut total = 0;
    if files.is_empty() || files.len() > FILE_COUNT {
        return Err("a bundle must contain 1 to 256 files".to_owned());
    }
    for file in files {
        if !safe_relative(&file.path)
            || !names.insert(file.path.to_lowercase())
            || file.path.eq_ignore_ascii_case(".jarvis-source.json")
        {
            return Err("unsafe, duplicate or reserved skill resource path".to_owned());
        }
        total += file.bytes.len();
        if file.bytes.len() > FILE_LIMIT || total > BUNDLE_LIMIT {
            return Err("bundle exceeds 4 MiB per file or 16 MiB total".to_owned());
        }
    }
    Ok(())
}

fn local_files(root: &Path) -> Result<Vec<BundleFile>, String> {
    fn walk(
        root: &Path,
        directory: &Path,
        files: &mut Vec<BundleFile>,
        total: &mut u64,
        entries: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err("skill resources exceed 32 directory levels".to_owned());
        }
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            *entries += 1;
            if *entries > 1024 {
                return Err("skill resources exceed 1024 filesystem entries".to_owned());
            }
            let path = entry.map_err(|error| error.to_string())?.path();
            let meta = ordinary(&path)?;
            if files.len() >= FILE_COUNT {
                return Err("bundle contains too many files".to_owned());
            }
            if meta.is_dir() {
                walk(root, &path, files, total, entries, depth + 1)?;
            } else if meta.is_file() {
                *total += meta.len();
                if meta.len() > FILE_LIMIT as u64 || *total > BUNDLE_LIMIT as u64 {
                    return Err("bundle exceeds file or total size limits".to_owned());
                }
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode() & 0o111 != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|error| error.to_string())?
                    .take(FILE_LIMIT as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                files.push(BundleFile {
                    path: relative,
                    bytes,
                    executable,
                });
            } else {
                return Err("skill resources must be regular files".to_owned());
            }
        }
        Ok(())
    }
    ordinary(root)?;
    let mut files = Vec::new();
    walk(root, root, &mut files, &mut 0, &mut 0, 0)?;
    validate_files(&files)?;
    Ok(files)
}

fn install(
    directory: &Path,
    name: &str,
    files: &[BundleFile],
    source: &Value,
) -> Result<Value, String> {
    if !valid_name(name) {
        return Err("invalid skill name".to_owned());
    }
    validate_files(files)?;
    let manifest = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or("bundle has no SKILL.md")?;
    let text = std::str::from_utf8(&manifest.bytes).map_err(|error| error.to_string())?;
    if manifest.bytes.len() as u64 > MANIFEST_LIMIT {
        return Err("SKILL.md exceeds 64 KiB".to_owned());
    }
    let metadata = metadata(text, name)?;
    let mut base = directory.to_path_buf();
    for component in [".agents", "skills"] {
        base.push(component);
        if !base.exists() {
            std::fs::create_dir(&base).map_err(|error| error.to_string())?;
        }
        if !ordinary(&base)?.is_dir()
            || !plain(&std::fs::canonicalize(&base).map_err(|error| error.to_string())?)
                .starts_with(directory)
        {
            return Err("installation directory is not an ordinary project folder".to_owned());
        }
    }
    let destination = base.join(name);
    if destination.symlink_metadata().is_ok() {
        return Err("skill already exists; no existing bundle was overwritten".to_owned());
    }
    let stage = base.join(format!(".install-{}", jarvis_core::DelegatedTaskId::new()));
    std::fs::create_dir(&stage).map_err(|error| error.to_string())?;
    let written = (|| {
        for file in files {
            let path = stage.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&path, &file.bytes).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|error| error.to_string())?;
            }
            #[cfg(not(unix))]
            let _ = file.executable;
        }
        std::fs::write(stage.join(".jarvis-source.json"), source.to_string())
            .map_err(|error| error.to_string())?;
        if destination.symlink_metadata().is_ok() {
            return Err("destination appeared during installation; nothing overwritten".to_owned());
        }
        std::fs::rename(&stage, &destination).map_err(|error| error.to_string())?;
        Ok(
            json!({"installed":true,"metadata":metadata,"directory":destination.to_string_lossy(),"source":source,"dependencies_installed":false,"note":"Bundle installed, not executed. Activate by name in delegation; required packages need authorized project-local commands and account connections need their own setup."}),
        )
    })();
    if stage.exists()
        && plain(&std::fs::canonicalize(&stage).map_err(|error| error.to_string())?).parent()
            == Some(base.as_path())
    {
        let _ = std::fs::remove_dir_all(&stage);
    }
    written
}

#[derive(Deserialize)]
struct Tree {
    tree: Vec<Entry>,
    truncated: bool,
}
#[derive(Deserialize)]
struct Entry {
    path: String,
    mode: String,
    sha: String,
    size: Option<usize>,
}

pub(crate) struct SkillBundles {
    folders: CommandRunTool,
    client: Option<reqwest::Client>,
}

impl SkillBundles {
    pub(crate) fn new(roots: &[PathBuf]) -> Result<Self, CommandRunToolError> {
        Ok(Self {
            folders: CommandRunTool::new(roots)?,
            client: reqwest::Client::builder()
                .user_agent("jarvis-skill-bundles/1.0")
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .ok(),
        })
    }

    pub(crate) fn definitions() -> Result<Vec<ToolDefinition>, CommandRunToolError> {
        let discover = r#"{"type":"object","additionalProperties":false,"properties":{"directory":{"type":"string"},"repository":{"type":"string","description":"Optional public GitHub owner/repo to discover remotely."},"revision":{"type":"string","description":"Branch, tag or commit to resolve; defaults to HEAD."}}}"#;
        let install = r#"{"type":"object","additionalProperties":false,"required":["name"],"properties":{"directory":{"type":"string"},"name":{"type":"string"},"source":{"type":"string","description":"Local bundle directory within permitted folders. Mutually exclusive with repository."},"repository":{"type":"string","description":"Public GitHub owner/repo from discovery."},"revision":{"type":"string","description":"Full immutable 40-character commit SHA from discovery."},"path":{"type":"string","description":"Bundle path from discovery, or . for repository root."}}}"#;
        Ok(vec![
            definition(
                "jarvis.skills.discover",
                "Discover coding skill bundles",
                "Lists project Agent Skills bundles, including compatibility and invalid manifests; optional public GitHub repository discovery resolves a ref to a pinned commit and SKILL.md paths. No scripts execute. Native jarvis.skill procedures are separate.",
                discover,
                ToolEffect::ExternalCommunication,
                2,
            )?,
            definition(
                "jarvis.skills.install",
                "Install a project skill bundle",
                "Copies a complete local or pinned public GitHub Agent Skills bundle with its resources into the selected permitted project's .agents/skills. No Git prerequisite, overwrite, scripts, global installation, credentials or permission grants. Dependency installation is separate authorized host execution. Activate installed names with the coding task's skills array.",
                install,
                ToolEffect::Write,
                2,
            )?,
        ])
    }

    async fn get(
        &self,
        repository: &str,
        suffix: &[&str],
        raw: bool,
        limit: usize,
    ) -> Result<Vec<u8>, AdapterError> {
        let components: Vec<_> = repository.split('/').collect();
        if components.len() != 2
            || components.iter().any(|part| {
                part.is_empty()
                    || *part == "."
                    || *part == ".."
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            })
        {
            return Err(refused(
                "repository must be a public GitHub owner/repo, not a URL",
            ));
        }
        let mut url = reqwest::Url::parse("https://api.github.com/repos/")
            .map_err(|error| refused(error.to_string()))?;
        url.path_segments_mut()
            .map_err(|()| refused("invalid GitHub base"))?
            .pop_if_empty()
            .extend(components)
            .extend(suffix);
        if suffix.contains(&"trees") {
            url.query_pairs_mut().append_pair("recursive", "1");
        }
        let accept = if raw {
            "application/vnd.github.raw+json"
        } else {
            "application/vnd.github+json"
        };
        let mut response = self
            .client
            .as_ref()
            .ok_or_else(|| {
                refused("secure GitHub HTTP client unavailable; local bundles remain usable")
            })?
            .get(url)
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", "2026-03-10")
            .send()
            .await
            .map_err(|error| refused(format!("GitHub unavailable: {error}")))?;
        if !response.status().is_success() {
            return Err(refused(format!(
                "GitHub returned HTTP {}; private repositories, rate limits or missing refs may require manual setup. No credentials forwarded.",
                response.status()
            )));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| refused(error.to_string()))?
        {
            if bytes.len() + chunk.len() > limit {
                return Err(refused("GitHub response exceeds the transfer limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn tree(&self, repository: &str, revision: &str) -> Result<(String, Tree), AdapterError> {
        let commit: Value = serde_json::from_slice(
            &self
                .get(repository, &["commits", revision], false, 4 * 1024 * 1024)
                .await?,
        )
        .map_err(|error| refused(error.to_string()))?;
        let sha = commit["sha"]
            .as_str()
            .filter(|text| valid_sha(text))
            .ok_or_else(|| refused("GitHub did not return a commit SHA"))?
            .to_owned();
        let tree_sha = commit["commit"]["tree"]["sha"]
            .as_str()
            .filter(|text| valid_sha(text))
            .ok_or_else(|| refused("GitHub did not return a tree SHA"))?;
        let tree: Tree = serde_json::from_slice(
            &self
                .get(
                    repository,
                    &["git", "trees", tree_sha],
                    false,
                    8 * 1024 * 1024,
                )
                .await?,
        )
        .map_err(|error| refused(error.to_string()))?;
        if tree.truncated {
            return Err(refused(
                "GitHub tree was truncated; select a smaller repository or install a local bundle instead",
            ));
        }
        Ok((sha, tree))
    }

    async fn remote_files(
        &self,
        repository: &str,
        value: &Value,
    ) -> Result<(Vec<BundleFile>, Value), AdapterError> {
        let revision = value["revision"]
            .as_str()
            .filter(|text| valid_sha(text))
            .ok_or_else(|| refused("install requires the full commit SHA returned by discovery"))?;
        let (actual, tree) = self.tree(repository, revision).await?;
        if !actual.eq_ignore_ascii_case(revision) {
            return Err(refused("resolved commit differs from requested pin"));
        }
        let path = value["path"].as_str().unwrap_or(".");
        if path != "." && !safe_relative(path) {
            return Err(refused("unsafe bundle source path"));
        }
        let prefix = if path == "." {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut files = Vec::new();
        let mut total = 0;
        for entry in tree.tree {
            let Some(relative) = entry.path.strip_prefix(&prefix) else {
                continue;
            };
            if entry.mode == "040000" {
                continue;
            }
            if !matches!(entry.mode.as_str(), "100644" | "100755")
                || !safe_relative(relative)
                || !valid_sha(&entry.sha)
            {
                return Err(refused("bundle contains unsafe paths, links or submodules"));
            }
            let size = entry
                .size
                .ok_or_else(|| refused("GitHub blob size missing"))?;
            total += size;
            if size > FILE_LIMIT || total > BUNDLE_LIMIT || files.len() >= FILE_COUNT {
                return Err(refused("bundle exceeds resource limits"));
            }
            let bytes = self
                .get(repository, &["git", "blobs", &entry.sha], true, FILE_LIMIT)
                .await?;
            if bytes.len() != size {
                return Err(refused("GitHub blob size differs from its tree entry"));
            }
            files.push(BundleFile {
                path: relative.to_owned(),
                bytes,
                executable: entry.mode == "100755",
            });
        }
        Ok((
            files,
            json!({"repository":repository,"revision":actual,"path":path}),
        ))
    }

    async fn run(&self, tool: &str, value: &Value) -> Result<Value, AdapterError> {
        let directory = self
            .folders
            .resolve_directory(value["directory"].as_str().unwrap_or("."))
            .map_err(refused)?;
        let repository = value["repository"].as_str();
        if tool == "jarvis.skills.discover" {
            if let Some(repository) = repository {
                let (revision, tree) = self
                    .tree(repository, value["revision"].as_str().unwrap_or("HEAD"))
                    .await?;
                let paths: Vec<_> = tree
                    .tree
                    .iter()
                    .filter(|entry| entry.path == "SKILL.md" || entry.path.ends_with("/SKILL.md"))
                    .map(|entry| entry.path.clone())
                    .collect();
                if paths.len() > CATALOG_LIMIT {
                    return Err(refused(
                        "repository contains too many skills; use a narrower source",
                    ));
                }
                return Ok(
                    json!({"repository":repository,"revision":revision,"manifests":paths,"note":"Choose a bundle path and install this exact commit. Discovery is not installation or a trust endorsement."}),
                );
            }
            return catalog(&directory).map_err(refused);
        }
        let name = value["name"]
            .as_str()
            .filter(|name| valid_name(name))
            .ok_or_else(|| refused("a valid skill name is required"))?;
        let (files, source) = match (repository, value["source"].as_str()) {
            (None, Some(source)) => {
                let root = self.folders.resolve_directory(source).map_err(refused)?;
                (
                    local_files(&root).map_err(refused)?,
                    json!({"local":root.to_string_lossy()}),
                )
            }
            (Some(repository), None) => self.remote_files(repository, value).await?,
            _ => {
                return Err(refused(
                    "provide exactly one local source or public GitHub repository",
                ));
            }
        };
        install(&directory, name, &files, &source).map_err(refused)
    }
}

fn valid_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[async_trait]
impl ToolExecutor for SkillBundles {
    fn adapter_id(&self) -> &'static str {
        "coding-skill-bundles"
    }
    async fn execute(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolCallResult, AdapterError> {
        if request.is_past_deadline(UtcTimestamp::now(&SystemClock)) {
            return Err(refused("call expired before execution"));
        }
        let tool = request.tool().to_string();
        if !matches!(
            tool.as_str(),
            "jarvis.skills.discover" | "jarvis.skills.install"
        ) {
            return Err(AdapterError::NotImplemented { tool });
        }
        self.run(&tool, request.arguments()).await.map(result)
    }
}

#[cfg(test)]
#[path = "skill_bundles_tests.rs"]
mod tests;
