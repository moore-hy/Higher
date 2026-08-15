//! BATCH-02.1 / DEV-0022 - Higher Runtime Sandbox 路径 Guard 测试。
//!
//! 覆盖任务 §54 全部 12 项（AI 工具 2 项在 ai_panel.rs）：
//! 1. 正常 relative attachment path 可解析
//! 2. ../escape.png 被拒绝
//! 3. ..\escape.png 被拒绝
//! 4. C:\Windows\test.txt 作为 relative_path 被拒绝
//! 5. D:\test.txt 被拒绝
//! 6. UNC path 被拒绝
//! 7. sandbox 内删除允许（路径解析成功）
//! 8. sandbox 外删除拒绝（路径越界）
//! 9. 数据库恶意 relative_path 无法逃逸（含混合分隔 / 编码）
//! 12. 不同 Profile attachment 继续隔离（Repository 层）

use app_lib::sandbox;
use std::path::PathBuf;

fn sandbox_root() -> PathBuf {
    // 真实临时目录作为 sandbox root（canonicalize 需要存在）
    let dir = std::env::temp_dir().join(format!("higher_sandbox_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_normal_relative_path_resolves() {
    let root = sandbox_root();
    // 模拟已存在的多级附件文件
    let rel = format!("1/9/42/{}.png", "abc");
    let full = root.join(&rel);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, b"png").unwrap();

    let resolved = sandbox::resolve_in_sandbox(&root, &rel).unwrap();
    assert!(resolved.starts_with(root.canonicalize().unwrap()));
    assert!(resolved.ends_with("abc.png"));

    // 不存在的合法路径同样可解析（父目录存在与否不影响静态校验）
    assert!(sandbox::resolve_in_sandbox(&root, "1/9/43/new.png").is_ok());
}

#[test]
fn test_dotdot_slash_escape_rejected() {
    let root = sandbox_root();
    assert!(sandbox::resolve_in_sandbox(&root, "../escape.png").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "1/../../escape.png").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "a/b/../../../c.png").is_err());
}

#[test]
fn test_dotdot_backslash_escape_rejected() {
    let root = sandbox_root();
    assert!(sandbox::resolve_in_sandbox(&root, "..\\escape.png").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "1\\..\\..\\escape.png").is_err());
}

#[test]
fn test_windows_abs_path_c_drive_rejected() {
    let root = sandbox_root();
    assert!(sandbox::resolve_in_sandbox(&root, "C:\\Windows\\test.txt").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "C:/Windows/test.txt").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "c:\\windows\\test.txt").is_err());
}

#[test]
fn test_windows_abs_path_d_drive_rejected() {
    let root = sandbox_root();
    assert!(sandbox::resolve_in_sandbox(&root, "D:\\test.txt").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "D:/test.txt").is_err());
    // 任意盘符（X:）
    assert!(sandbox::resolve_in_sandbox(&root, "X:whatever").is_err());
}

#[test]
fn test_unc_path_rejected() {
    let root = sandbox_root();
    assert!(sandbox::resolve_in_sandbox(&root, "\\\\server\\share\\file.png").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "\\server\\share").is_err());
    // Unix 绝对路径
    assert!(sandbox::resolve_in_sandbox(&root, "/etc/passwd").is_err());
    // 空路径
    assert!(sandbox::resolve_in_sandbox(&root, "  ").is_err());
}

#[test]
fn test_delete_inside_sandbox_allowed() {
    let root = sandbox_root();
    let rel = format!("1/9/42/del_{}.tmp", std::process::id());
    let full = root.join(&rel);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, b"x").unwrap();

    let resolved = sandbox::resolve_in_sandbox(&root, &rel).unwrap();
    assert!(resolved.starts_with(root.canonicalize().unwrap()));
    std::fs::remove_file(resolved).unwrap();
    assert!(!full.exists());
}

#[test]
fn test_delete_outside_sandbox_rejected() {
    let root = sandbox_root();
    // 任何越界路径都无法解析 → 删除流程（command 层）不会执行 remove_file
    assert!(sandbox::resolve_in_sandbox(&root, "../outside.txt").is_err());
    assert!(sandbox::resolve_in_sandbox(&root, "C:\\Windows\\System32\\cmd.exe").is_err());
    // root 外的真实文件（temp 根下）也无法通过 relative 形式指向
    let outside = std::env::temp_dir().join(format!("higher_outside_{}.txt", std::process::id()));
    std::fs::write(&outside, b"secret").unwrap();
    // 尝试用 .. 指向它
    let rel = format!("../higher_outside_{}.txt", std::process::id());
    assert!(sandbox::resolve_in_sandbox(&root, &rel).is_err());
    assert!(outside.exists(), "sandbox 外文件未被触碰");
    std::fs::remove_file(&outside).ok();
}

#[test]
fn test_malicious_db_relative_path_cannot_escape() {
    let root = sandbox_root();
    // 混合分隔符 / URL 编码变体 / 末尾 .. 等均拒绝
    for evil in [
        "..%2f..%2fescape.png",
        "1/2/../../..\\escape.png",
        "./../escape.png",
        "1//../escape.png",
        "…/escape.png",      // 非法字符不构成 ..，但也无法指向 root 外
        "1/2/%2e%2e/escape",
    ] {
        let r = sandbox::resolve_in_sandbox(&root, evil);
        // 要么被拒绝；要么解析结果仍在 root 内（绝不越界）
        match r {
            Err(_) => {}
            Ok(p) => assert!(p.starts_with(root.canonicalize().unwrap()), "越界：{} → {:?}", evil, p),
        }
    }
}

#[test]
fn test_import_source_must_be_single_file() {
    // 用户主动导入的唯一例外：必须是已存在的具体文件
    let file = std::env::temp_dir().join(format!("higher_import_{}.png", std::process::id()));
    std::fs::write(&file, b"png").unwrap();
    assert!(sandbox::resolve_import_source(file.to_str().unwrap()).is_ok());

    // 目录拒绝
    let dir = std::env::temp_dir();
    assert!(sandbox::resolve_import_source(dir.to_str().unwrap()).is_err());
    // 不存在拒绝
    assert!(sandbox::resolve_import_source("Z:\\no\\such\\file.png").is_err());
    // 空
    assert!(sandbox::resolve_import_source("  ").is_err());

    std::fs::remove_file(&file).ok();
}

#[test]
fn test_profile_attachment_isolation_still_enforced() {
    // 沿用 attachments.rs 的 Repository 隔离断言（快捷回归）
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();

    use app_lib::repository::{
        attachment::AttachmentRepository, goal::GoalRepository,
        learning_item::LearningItemRepository, study_profile::StudyProfileRepository,
    };
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let att_repo = AttachmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let ga = goal_repo.create(pa.id, "GA", None).unwrap();
    let gb = goal_repo.create(pb.id, "GB", None).unwrap();
    let ia = item_repo.create_root(ga.id, "IA", None).unwrap();
    let ib = item_repo.create_root(gb.id, "IB", None).unwrap();

    let att_a = att_repo
        .create(pa.id, Some(ia.id), None, "image", "a.png", "1/1/1/a.png", None, "")
        .unwrap();
    // B 无法读取/列出 A 的附件（list 按 item；跨 Profile create 已被拒）
    assert!(att_repo.create(pb.id, Some(ia.id), None, "image", "x.png", "x.png", None, "").is_err());
    let b_list = att_repo.list_by_learning_item(ib.id).unwrap();
    assert_eq!(b_list.len(), 0);
    // A 的附件仍归属 A
    assert_eq!(att_repo.list_by_learning_item(ia.id).unwrap().len(), 1);
    let _ = att_a;
}
