//! Optional network acceptance; every installed skill and Agent link stays in a temporary home.
use power_switch::{
    paths::{Paths, Settings},
    skills::{self, InstallSource, Scope, SkillManager},
};

/// Download a public source, install one reviewed package, link it to isolated WorkBuddy and remove it.
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .ok_or("用法：skills_acceptance HTTPS_REPOSITORY [SUBDIR]")?;
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let paths = Paths {
        home: temp.path().join("home"),
        data: temp.path().join("app"),
        codex_env: None,
        workbuddy_env: None,
    };
    std::fs::create_dir_all(&paths.home).map_err(|e| e.to_string())?;
    let roots = skills::locations(&paths, &Settings::default())?;
    let mut manager = SkillManager::default();
    let preview = manager.preview_install(
        &roots,
        &paths.data,
        InstallSource::Git {
            url,
            subdir: args.next(),
        },
    )?;
    let candidate = preview.skills.first().ok_or("没有可安装技能")?;
    let id = candidate.folder.clone();
    manager.commit(&roots, &preview.token, &[candidate.index])?;
    let link = manager.preview_link(&roots, &id, &[Scope::Workbuddy])?;
    manager.commit(&roots, &link.token, &[])?;
    let groups = skills::list(&roots);
    assert!(groups
        .iter()
        .find(|g| g.scope == Scope::Workbuddy)
        .unwrap()
        .skills
        .iter()
        .any(|s| s.id == id && s.linked));
    let deletion = manager.preview_delete(&roots, Scope::Global, &id)?;
    assert_eq!(deletion.paths.len(), 2);
    manager.commit(&roots, &deletion.token, &[])?;
    assert!(skills::list(&roots)
        .iter()
        .all(|g| g.skills.is_empty() && g.error.is_none()));
    println!(
        "通过：HTTPS 下载 → 全局安装 → WorkBuddy 软链接 → 全局删除并清理引用。全部位于临时目录。"
    );
    Ok(())
}
