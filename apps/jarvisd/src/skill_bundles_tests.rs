#![allow(clippy::unwrap_used)]
use super::*;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("jsb-{}", jarvis_core::scratch_tag()));
        std::fs::create_dir_all(&path).unwrap();
        Self(plain(&std::fs::canonicalize(path).unwrap()))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}
fn manifest() -> Vec<u8> {
    b"---\nname: web-design\ndescription: >-\n  Build coherent websites.\n  Verify their behavior.\ncompatibility: Requires Node\n---\nUse resources and run tests.\n".to_vec()
}
fn files() -> Vec<BundleFile> {
    vec![
        BundleFile {
            path: "SKILL.md".to_owned(),
            bytes: manifest(),
            executable: false,
        },
        BundleFile {
            path: "references/design.md".to_owned(),
            bytes: b"design guidance".to_vec(),
            executable: false,
        },
    ]
}

#[test]
fn definitions_are_valid_and_installation_requires_owner_approval() {
    let definitions = SkillBundles::definitions().unwrap();
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[1].approval(), ApprovalPolicy::Ask);
}

#[test]
fn full_resources_install_and_activate_without_executing_scripts() {
    let scratch = Scratch::new();
    let answer = install(
        &scratch.0,
        "web-design",
        &files(),
        &json!({"revision":"pinned"}),
    )
    .unwrap();
    assert_eq!(answer["installed"], true);
    assert_eq!(answer["dependencies_installed"], false);
    assert!(
        scratch
            .0
            .join(".agents/skills/web-design/references/design.md")
            .exists()
    );
    let prompt = context(&scratch.0, &["web-design".to_owned()]).unwrap();
    assert!(prompt.contains("Use resources and run tests."));
    assert!(prompt.contains("Requires Node"));
    assert!(install(&scratch.0, "web-design", &files(), &json!({})).is_err());
    assert!(context(&scratch.0, &["missing".to_owned()]).is_err());
}

#[test]
fn unsafe_resource_paths_and_case_collisions_fail_closed() {
    for path in [
        "../escape",
        "/absolute",
        "a/../../escape",
        "a\\b",
        "C:/escape",
        "file:stream",
        "CON.txt",
        "aux",
        "trailing.",
        "a//b",
        ".jarvis-source.json",
    ] {
        let scratch = Scratch::new();
        let mut bundle = files();
        bundle.push(BundleFile {
            path: path.to_owned(),
            bytes: vec![1],
            executable: false,
        });
        assert!(
            install(&scratch.0, "web-design", &bundle, &json!({})).is_err(),
            "{path}"
        );
        assert!(!scratch.0.join(".agents").exists());
    }
    let mut bundle = files();
    bundle.push(BundleFile {
        path: "skill.md".to_owned(),
        bytes: vec![1],
        executable: false,
    });
    assert!(validate_files(&bundle).is_err());
}

#[test]
fn malformed_and_oversized_manifests_are_not_installed() {
    for bytes in [
        b"name: web-design".to_vec(),
        b"---\nname: wrong\ndescription: test\n---\n".to_vec(),
        b"---\nname: web-design\ndescription: ''\n---\n".to_vec(),
        vec![b'a'; usize::try_from(MANIFEST_LIMIT).unwrap() + 1],
    ] {
        let scratch = Scratch::new();
        let bundle = [BundleFile {
            path: "SKILL.md".to_owned(),
            bytes,
            executable: false,
        }];
        assert!(install(&scratch.0, "web-design", &bundle, &json!({})).is_err());
    }
}

#[tokio::test]
async fn local_install_uses_the_default_settings_folder_not_a_profile_worktree() {
    let scratch = Scratch::new();
    let source = scratch.0.join("source");
    std::fs::create_dir_all(source.join("references")).unwrap();
    std::fs::write(source.join("SKILL.md"), manifest()).unwrap();
    std::fs::write(source.join("references/design.md"), "resources").unwrap();
    let tool = SkillBundles::new(std::slice::from_ref(&scratch.0)).unwrap();
    let answer = tool
        .run(
            "jarvis.skills.install",
            &json!({"name":"web-design","source":"source"}),
        )
        .await
        .unwrap();
    assert!(
        answer["directory"]
            .as_str()
            .unwrap()
            .starts_with(scratch.0.to_str().unwrap())
    );
    assert!(
        tool.run(
            "jarvis.skills.install",
            &json!({"name":"web-design","source":"../escape"})
        )
        .await
        .is_err()
    );
    assert!(
        tool.run(
            "jarvis.skills.install",
            &json!({"name":"web-design","repository":"https://evil.invalid/a/b","revision":"main"})
        )
        .await
        .is_err()
    );
}

#[test]
fn linked_resources_fail_closed() {
    let scratch = Scratch::new();
    let source = scratch.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("SKILL.md"), manifest()).unwrap();
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(source.join("linked"))
        .arg(std::env::temp_dir())
        .output()
        .unwrap()
        .status
        .success();
    #[cfg(unix)]
    let status = std::os::unix::fs::symlink(std::env::temp_dir(), source.join("linked")).is_ok();
    assert!(status, "test must establish a real link");
    assert!(local_files(&source).is_err());
    #[cfg(windows)]
    std::fs::remove_dir(source.join("linked")).unwrap();
}

#[test]
fn frontmatter_requires_an_exact_closing_delimiter_and_reserved_names_ignore_case() {
    assert!(
        metadata(
            "---
name: web-design
description: ok
---not-a-delimiter
body",
            "web-design"
        )
        .is_err()
    );
    let mut reserved = files();
    reserved.push(BundleFile {
        path: ".JARVIS-SOURCE.JSON".to_owned(),
        bytes: vec![1],
        executable: false,
    });
    assert!(validate_files(&reserved).is_err());
}

#[test]
fn empty_directory_floods_are_bounded_before_installation() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("SKILL.md"), manifest()).unwrap();
    for index in 0..1025 {
        std::fs::create_dir(scratch.0.join(format!("empty-{index}"))).unwrap();
    }
    assert!(
        local_files(&scratch.0)
            .err()
            .unwrap()
            .contains("filesystem entries")
    );
}

#[tokio::test]
#[ignore = "Read-only public GitHub API requests; installs a pinned bundle only into a scratch directory, executes no scripts"]
async fn public_github_discovery_and_immutable_bundle_install_work_live() {
    let scratch = Scratch::new();
    let tool = SkillBundles::new(std::slice::from_ref(&scratch.0)).unwrap();
    let found = tool
        .run(
            "jarvis.skills.discover",
            &json!({"repository":"anthropics/skills"}),
        )
        .await
        .unwrap();
    let revision = found["revision"].as_str().unwrap();
    assert!(valid_sha(revision));
    assert!(
        found["manifests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "skills/frontend-design/SKILL.md")
    );
    let installed = tool.run("jarvis.skills.install", &json!({"repository":"anthropics/skills","revision":revision,"path":"skills/frontend-design","name":"frontend-design"})).await.unwrap();
    assert_eq!(installed["installed"], true);
    assert_eq!(installed["dependencies_installed"], false);
    let provenance: Value = serde_json::from_str(
        &std::fs::read_to_string(
            scratch
                .0
                .join(".agents/skills/frontend-design/.jarvis-source.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(provenance["revision"], revision);
    assert!(
        context(&scratch.0, &["frontend-design".to_owned()])
            .unwrap()
            .contains("frontend-design")
    );
}
