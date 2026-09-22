use power_switch::{
    paths::{Paths, Settings},
    skills::{self, InstallSource, Location, Scope, SkillManager},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Isolate every skill root, app staging directory and source away from the user's installed skills.
fn fixture() -> (tempfile::TempDir, Paths, Vec<Location>, SkillManager) {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: temp.path().join("home"),
        data: temp.path().join("app"),
        codex_env: None,
        workbuddy_env: None,
    };
    fs::create_dir_all(&paths.home).unwrap();
    let roots = skills::locations(&paths, &Settings::default()).unwrap();
    (temp, paths, roots, SkillManager::default())
}

/// Create an ordinary skill package with Markdown and a resource file.
fn package(path: &Path, name: &str) {
    fs::create_dir_all(path.join("resources")).unwrap();
    fs::write(path.join("SKILL.md"), format!("---\nname: {name}\ndescription: |\n  第一行描述\n  第二行描述\n---\n\n# 使用说明\n\n不执行任何脚本。\n")).unwrap();
    fs::write(path.join("resources/example.txt"), "original resource").unwrap();
}

/// Fetch a fixture root using the same stable scope enum as the native commands.
fn at(roots: &[Location], scope: Scope) -> PathBuf {
    roots
        .iter()
        .find(|r| r.scope == scope)
        .unwrap()
        .path
        .clone()
}

/// Scan nested packages, native user roots and environment/setting directory overrides.
#[test]
fn inventories_details_and_overridden_locations() {
    let (_temp, mut paths, roots, _) = fixture();
    package(
        &at(&roots, Scope::Global).join("nested/example"),
        "示例技能",
    );
    package(
        &at(&roots, Scope::Codex).join(".system/internal"),
        "internal",
    );
    let groups = skills::list(&roots);
    assert_eq!(groups[0].skills[0].id, "nested/example");
    assert!(groups[0].skills[0].description.contains("第二行描述"));
    assert_eq!(groups[2].skills[0].id, ".system/internal");
    assert!(skills::detail(&roots, Scope::Global, "nested/example")
        .unwrap()
        .document
        .contains("# 使用说明"));
    paths.codex_env = Some(paths.home.join("portable-codex"));
    paths.workbuddy_env = Some(paths.home.join("portable-wb"));
    let settings = Settings {
        claude_path: Some(paths.home.join("custom-claude/settings.json")),
        ..Settings::default()
    };
    let updated = skills::locations(&paths, &settings).unwrap();
    assert_eq!(
        at(&updated, Scope::Codex),
        paths.home.join("portable-codex/skills")
    );
    assert_eq!(
        at(&updated, Scope::Workbuddy),
        paths.home.join("portable-wb/skills")
    );
    assert_eq!(
        at(&updated, Scope::Claude),
        paths.home.join("custom-claude/skills")
    );
}

/// Installation previews are inert, cancellation releases staging, and commits use reviewed bytes.
#[test]
fn install_snapshots_only_to_global_after_confirmation() {
    let (temp, paths, roots, mut manager) = fixture();
    let source = temp.path().join("source/example");
    package(&source, "example");
    let preview = manager
        .preview_install(
            &roots,
            &paths.data,
            InstallSource::Local {
                path: source.join("SKILL.md"),
            },
        )
        .unwrap();
    assert!(!at(&roots, Scope::Global).exists());
    manager.cancel(&preview.token);
    assert!(manager.commit(&roots, &preview.token, &[0]).is_err());
    assert_eq!(
        fs::read_dir(paths.data.join("skill-staging"))
            .unwrap()
            .count(),
        0
    );
    let preview = manager
        .preview_install(
            &roots,
            &paths.data,
            InstallSource::Local {
                path: source.clone(),
            },
        )
        .unwrap();
    fs::write(
        source.join("resources/example.txt"),
        "changed after preview",
    )
    .unwrap();
    manager.commit(&roots, &preview.token, &[0]).unwrap();
    assert_eq!(
        fs::read_to_string(at(&roots, Scope::Global).join("example/resources/example.txt"))
            .unwrap(),
        "original resource"
    );
    for scope in [Scope::Claude, Scope::Codex, Scope::Workbuddy] {
        assert!(!at(&roots, scope).exists());
    }
    let duplicate = manager
        .preview_install(&roots, &paths.data, InstallSource::Local { path: source })
        .unwrap();
    assert!(duplicate.skills[0].exists);
    assert!(manager.commit(&roots, &duplicate.token, &[0]).is_err());
}

/// A multi-package preview imports only selected entries, and malformed sources cannot escape staging.
#[test]
fn batch_selection_invalid_paths_and_missing_documents() {
    let (temp, paths, roots, mut manager) = fixture();
    let source = temp.path().join("repo");
    package(&source.join("one"), "one");
    package(&source.join("two"), "two");
    let preview = manager
        .preview_install(&roots, &paths.data, InstallSource::Local { path: source })
        .unwrap();
    assert_eq!(preview.skills.len(), 2);
    let index = preview
        .skills
        .iter()
        .find(|s| s.folder == "two")
        .unwrap()
        .index;
    manager.commit(&roots, &preview.token, &[index]).unwrap();
    assert!(!at(&roots, Scope::Global).join("one").exists());
    assert!(at(&roots, Scope::Global).join("two").exists());
    for id in ["../escape", "/absolute", "x/../../escape", "CON", "a\\b"] {
        assert!(skills::detail(&roots, Scope::Global, id).is_err());
        assert!(manager.preview_delete(&roots, Scope::Global, id).is_err());
    }
    let invalid = temp.path().join("invalid");
    fs::create_dir(&invalid).unwrap();
    assert!(manager
        .preview_install(&roots, &paths.data, InstallSource::Local { path: invalid })
        .is_err());
    assert!(manager
        .preview_install(
            &roots,
            &paths.data,
            InstallSource::Git {
                url: "file:///tmp/repo".into(),
                subdir: None
            }
        )
        .is_err());
    assert!(manager
        .preview_install(
            &roots,
            &paths.data,
            InstallSource::Git {
                url: "https://user:secret@example.com/repo".into(),
                subdir: None
            }
        )
        .is_err());
    assert!(manager
        .preview_install(
            &roots,
            &paths.data,
            InstallSource::Git {
                url: "https://example.com/repo".into(),
                subdir: Some("../escape".into())
            }
        )
        .is_err());
}

/// Skill edits or target-setting changes invalidate a pending destructive confirmation.
#[test]
fn deletion_rechecks_files_and_target_preferences() {
    let (_temp, _paths, roots, mut manager) = fixture();
    let source = at(&roots, Scope::Global).join("example");
    package(&source, "example");
    let preview = manager
        .preview_delete(&roots, Scope::Global, "example")
        .unwrap();
    fs::write(source.join("resources/example.txt"), "user update").unwrap();
    assert!(manager.commit(&roots, &preview.token, &[]).is_err());
    assert!(source.exists());
    let preview = manager
        .preview_delete(&roots, Scope::Global, "example")
        .unwrap();
    let mut moved = roots.clone();
    moved[0].path = moved[0].path.join("different");
    assert!(manager.commit(&moved, &preview.token, &[]).is_err());
    let preview = manager
        .preview_delete(&roots, Scope::Global, "example")
        .unwrap();
    manager.commit(&roots, &preview.token, &[]).unwrap();
    assert!(!source.exists());
    assert!(skills::list(&roots)[0].skills.is_empty());
    assert!(at(&roots, Scope::Global)
        .join(".power-switch-trash")
        .is_dir());
}

/// Symlink tests use Unix primitives; Windows creation additionally requires OS privileges.
#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::symlink;

    /// Agent deletion removes only its link; global deletion removes reviewed references as well.
    #[test]
    fn global_linking_and_deletion_preserve_other_skills() {
        let (_temp, _paths, roots, mut manager) = fixture();
        let global = at(&roots, Scope::Global);
        package(&global.join("example"), "example");
        package(&global.join("keep"), "keep");
        let preview = manager
            .preview_link(
                &roots,
                "example",
                &[Scope::Claude, Scope::Codex, Scope::Workbuddy],
            )
            .unwrap();
        assert!(!at(&roots, Scope::Claude).exists());
        manager.commit(&roots, &preview.token, &[]).unwrap();
        for scope in [Scope::Claude, Scope::Codex, Scope::Workbuddy] {
            assert!(fs::symlink_metadata(at(&roots, scope).join("example"))
                .unwrap()
                .file_type()
                .is_symlink());
        }
        assert!(skills::list(&roots)[1].skills[0].linked);
        assert!(manager
            .preview_link(&roots, "example", &[Scope::Claude])
            .is_err());
        let preview = manager
            .preview_delete(&roots, Scope::Claude, "example")
            .unwrap();
        assert_eq!(preview.paths.len(), 1);
        manager.commit(&roots, &preview.token, &[]).unwrap();
        assert!(global.join("example/SKILL.md").is_file());
        assert!(at(&roots, Scope::Codex).join("example").exists());
        let preview = manager
            .preview_delete(&roots, Scope::Global, "example")
            .unwrap();
        assert_eq!(preview.paths.len(), 3);
        manager.commit(&roots, &preview.token, &[]).unwrap();
        assert!(fs::symlink_metadata(at(&roots, Scope::Codex).join("example")).is_err());
        assert!(global.join("keep/SKILL.md").is_file());
    }

    /// New references arriving after a delete preview require a new confirmation.
    #[test]
    fn late_global_reference_invalidates_deletion() {
        let (_temp, _paths, roots, mut manager) = fixture();
        let source = at(&roots, Scope::Global).join("example");
        package(&source, "example");
        let preview = manager
            .preview_delete(&roots, Scope::Global, "example")
            .unwrap();
        fs::create_dir_all(at(&roots, Scope::Claude)).unwrap();
        symlink(&source, at(&roots, Scope::Claude).join("late")).unwrap();
        assert!(manager.commit(&roots, &preview.token, &[]).is_err());
        assert!(source.exists());
    }

    /// Redirected parent links and redirected trash folders must never mutate their external targets.
    #[test]
    fn external_links_trash_redirection_and_root_changes_are_guarded() {
        let (temp, paths, roots, mut manager) = fixture();
        let outside = temp.path().join("outside");
        package(&outside.join("child"), "outside");
        let global = at(&roots, Scope::Global);
        fs::create_dir_all(&global).unwrap();
        symlink(&outside, global.join("shared")).unwrap();
        assert!(manager
            .preview_delete(&roots, Scope::Global, "shared/child")
            .is_err());
        let src = temp.path().join("source");
        package(&src, "source");
        symlink(&outside, src.join("escape")).unwrap();
        assert!(manager
            .preview_install(&roots, &paths.data, InstallSource::Local { path: src })
            .is_err());
        package(&global.join("example"), "example");
        let preview = manager
            .preview_delete(&roots, Scope::Global, "example")
            .unwrap();
        symlink(&outside, global.join(".power-switch-trash")).unwrap();
        assert!(manager.commit(&roots, &preview.token, &[]).is_err());
        assert!(global.join("example").exists());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        let preview = manager
            .preview_link(&roots, "example", &[Scope::Claude])
            .unwrap();
        fs::create_dir_all(at(&roots, Scope::Claude).parent().unwrap()).unwrap();
        symlink(&outside, at(&roots, Scope::Claude)).unwrap();
        assert!(manager.commit(&roots, &preview.token, &[]).is_err());
        assert!(!outside.join("example").exists());
    }

    /// Parent creation is preflighted for every target, preventing a partial connection on error.
    #[test]
    fn invalid_later_agent_parent_leaves_no_earlier_link() {
        let (_temp, _paths, roots, mut manager) = fixture();
        package(&at(&roots, Scope::Global).join("example"), "example");
        let preview = manager
            .preview_link(&roots, "example", &[Scope::Claude, Scope::Codex])
            .unwrap();
        fs::create_dir_all(at(&roots, Scope::Codex).parent().unwrap()).unwrap();
        fs::write(at(&roots, Scope::Codex), "not a directory").unwrap();
        assert!(manager.commit(&roots, &preview.token, &[]).is_err());
        assert!(!at(&roots, Scope::Claude).join("example").exists());
    }

    /// Invalid links remain visible and can be removed without needing their former target.
    #[test]
    fn broken_links_and_cycles_are_bounded() {
        let (_temp, paths, roots, mut manager) = fixture();
        let base = at(&roots, Scope::Claude);
        fs::create_dir_all(&base).unwrap();
        symlink(paths.home.join("gone"), base.join("broken")).unwrap();
        symlink(&base, base.join("cycle")).unwrap();
        let groups = skills::list(&roots);
        assert_eq!(groups[1].skills.len(), 1);
        assert!(groups[1].skills[0].problem.is_some());
        let preview = manager
            .preview_delete(&roots, Scope::Claude, "broken")
            .unwrap();
        manager.commit(&roots, &preview.token, &[]).unwrap();
        assert!(fs::symlink_metadata(base.join("broken")).is_err());
    }
}
