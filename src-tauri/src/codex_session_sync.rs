//! 按需 Codex 会话同步（对齐 `Dailin521/codex-provider-sync` 的 `sync` 命令）。
//!
//! 切换 Codex provider 后，Codex 把每个会话的"归属 provider"缓存在多处：会话 rollout
//! 文件、`state_5.sqlite` 的 `threads` 表、以及 Codex Desktop 的 `.codex-global-state.json`
//! 工作区缓存。当这些缓存里的 `model_provider` 与当前 `~/.codex/config.toml` 的
//! `model_provider` 不一致时，历史会话就会在列表 / `/resume` 里"消失"。本模块把这些缓存
//! 统一改写成当前 provider（缺省回退 `"openai"`），让历史会话重新可见。
//!
//! 复用本项目已有底层：`config::atomic_write`、修改前/后 mtime+len 快照校验、备份工具、
//! `rusqlite`（`bundled`+`backup`）。只读/改写本机 `~/.codex` 数据，不碰 `auth.json`、
//! 不登录、不改 provider 切换 / 代理逻辑、不重命名任何目录或标识符。

use crate::codex_config::{get_codex_config_dir, read_codex_config_text};
use crate::codex_history_migration::{
    backup_codex_jsonl_file, backup_codex_state_db, codex_state_db_paths, collect_jsonl_files,
    ensure_codex_session_file_unchanged,
};
use crate::config::{atomic_write, copy_file, get_app_config_dir};
use crate::database::Database;
use crate::error::AppError;
use chrono::Local;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml_edit::DocumentMut;

const SYNC_NAME: &str = "codex-session-sync";
const DEFAULT_PROVIDER: &str = "openai";
const CODEX_STATE_DB_FILENAME: &str = "state_5.sqlite";
const GLOBAL_STATE_FILE_BASENAME: &str = ".codex-global-state.json";
const GLOBAL_STATE_BACKUP_FILE_BASENAME: &str = ".codex-global-state.json.bak";

/// JSONL 会话目录（与参考项目 `SESSION_DIRS` 一致）。第一项为活跃会话，第二项为归档。
const SESSION_DIRS_MAX_DEPTH: [(&str, u8); 2] = [("sessions", 8), ("archived_sessions", 4)];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSessionSyncOutcome {
    /// 同步目标 provider（来自 live config.toml，缺省 "openai"）。
    pub target_provider: String,
    /// 被改写 `model_provider` 的会话 JSONL 文件数。
    pub rewrote_jsonl_files: usize,
    /// `threads.model_provider` 被更新的行数。
    pub updated_state_rows: usize,
    /// `threads.has_user_event` 被修复的行数。
    pub updated_user_event_rows: usize,
    /// `threads.cwd` 被修复的行数。
    pub updated_cwd_rows: usize,
    /// 是否存在 `.codex-global-state.json`。
    pub workspace_roots_present: bool,
    /// 工作区缓存是否被写入。
    pub workspace_roots_updated: bool,
    /// 被跳过的项（被占用的会话文件、无法解析的全局状态等），用于排障。
    pub skipped: Vec<String>,
}

/// 入口：把会话 / state DB / 工作区缓存同步到当前 Codex provider。
pub fn sync_codex_sessions_to_current_provider() -> Result<CodexSessionSyncOutcome, AppError> {
    let codex_dir = get_codex_config_dir();
    let config_text = read_codex_config_text().unwrap_or_default();
    let target = current_model_provider(&config_text);
    let backup_root = sync_backup_root();
    let mut skipped: Vec<String> = Vec::new();

    // 1) 扫描会话文件：改写 provider，并收集 sqlite 修复所需的 cwd / user-event 统计。
    let mut files = Vec::new();
    for (sub, max_depth) in SESSION_DIRS_MAX_DEPTH {
        collect_jsonl_files(&codex_dir.join(sub), &mut files, 0, max_depth);
    }

    let mut rewrote_jsonl_files = 0usize;
    let mut thread_cwd_by_id: BTreeMap<String, String> = BTreeMap::new();
    let mut user_event_thread_ids: HashSet<String> = HashSet::new();

    for path in &files {
        match process_session_file(path, &codex_dir, &target, &backup_root) {
            Ok(info) => {
                if info.rewritten {
                    rewrote_jsonl_files += 1;
                }
                if let Some(id) = info.id {
                    if let Some(cwd) = info.cwd {
                        thread_cwd_by_id.insert(id.clone(), cwd);
                    }
                    if info.has_user_event {
                        user_event_thread_ids.insert(id);
                    }
                }
            }
            Err(err) => {
                // 正被运行中的 Codex 写入的会话文件可能触发快照/占用校验失败：跳过并继续
                // （对齐参考项目"跳过被占用的 rollout，继续其余"）。
                log::warn!("跳过 Codex 会话文件 {}: {err}", path.display());
                skipped.push(format!("session:{}", path.display()));
            }
        }
    }

    // 2) 更新 state_5.sqlite：provider + has_user_event + cwd 修复。
    let mut updated_state_rows = 0usize;
    let mut updated_user_event_rows = 0usize;
    let mut updated_cwd_rows = 0usize;
    for db_path in codex_state_db_paths(&codex_dir, &config_text) {
        if !db_path.exists() {
            continue;
        }
        let stats = update_state_db(
            &db_path,
            &codex_dir,
            &target,
            &thread_cwd_by_id,
            &user_event_thread_ids,
            &backup_root,
        )?;
        updated_state_rows += stats.provider_rows;
        updated_user_event_rows += stats.user_event_rows;
        updated_cwd_rows += stats.cwd_rows;
    }

    // 3) 同步 Codex Desktop 工作区缓存（尽力而为：文件缺失/损坏不致命）。
    let primary_db = codex_dir.join(CODEX_STATE_DB_FILENAME);
    let workspace = sync_workspace_roots(&codex_dir, &primary_db, &backup_root, &mut skipped)?;

    log::info!(
        "Codex 会话同步完成: target={target}, jsonl={rewrote_jsonl_files}, provider_rows={updated_state_rows}, user_event_rows={updated_user_event_rows}, cwd_rows={updated_cwd_rows}, ws_present={}, ws_updated={}, skipped={}",
        workspace.present,
        workspace.updated,
        skipped.len(),
    );

    Ok(CodexSessionSyncOutcome {
        target_provider: target,
        rewrote_jsonl_files,
        updated_state_rows,
        updated_user_event_rows,
        updated_cwd_rows,
        workspace_roots_present: workspace.present,
        workspace_roots_updated: workspace.updated,
        skipped,
    })
}

fn sync_backup_root() -> PathBuf {
    get_app_config_dir()
        .join("backups")
        .join(SYNC_NAME)
        .join(Local::now().format("%Y%m%d_%H%M%S").to_string())
}

/// 读取 live config.toml 的根 `model_provider`，空/缺失则回退 `"openai"`。
fn current_model_provider(config_text: &str) -> String {
    config_text
        .parse::<DocumentMut>()
        .ok()
        .and_then(|doc| {
            doc.get("model_provider")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| DEFAULT_PROVIDER.to_string())
}

// ---------------------------------------------------------------------------
// 会话 JSONL 改写
// ---------------------------------------------------------------------------

struct SessionFileInfo {
    rewritten: bool,
    id: Option<String>,
    cwd: Option<String>,
    has_user_event: bool,
}

fn process_session_file(
    path: &Path,
    codex_dir: &Path,
    target: &str,
    backup_root: &Path,
) -> Result<SessionFileInfo, AppError> {
    let metadata_before = fs::metadata(path).map_err(|e| AppError::io(path, e))?;
    let modified_before = metadata_before.modified().ok();
    let len_before = metadata_before.len();
    let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;

    let mut id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut has_user_event = false;

    let mut rewritten_text = String::with_capacity(content.len());
    let mut changed = false;

    for segment in content.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map(|l| (l, "\n"))
            .unwrap_or((segment, ""));

        // 解析一次，复用于元数据提取 + user-event 检测。
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if id.is_none() && is_session_meta(&value) {
                if let Some(payload) = value.get("payload").and_then(Value::as_object) {
                    id = payload
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(raw_cwd) = payload.get("cwd").and_then(Value::as_str) {
                        if !raw_cwd.trim().is_empty() {
                            cwd = Some(to_desktop_path(raw_cwd));
                        }
                    }
                }
            }
            if !has_user_event && record_has_user_event(&value) {
                has_user_event = true;
            }
        }

        if let Some(next_line) = rewrite_session_meta_line_to_target(line, target) {
            rewritten_text.push_str(&next_line);
            changed = true;
        } else {
            rewritten_text.push_str(line);
        }
        rewritten_text.push_str(newline);
    }

    if changed {
        ensure_codex_session_file_unchanged(path, modified_before, len_before)?;
        backup_codex_jsonl_file(path, codex_dir, backup_root)?;
        ensure_codex_session_file_unchanged(path, modified_before, len_before)?;
        atomic_write(path, rewritten_text.as_bytes())?;
    }

    Ok(SessionFileInfo {
        rewritten: changed,
        id,
        cwd,
        has_user_event,
    })
}

fn is_session_meta(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("session_meta")
        && value.get("payload").map(Value::is_object).unwrap_or(false)
}

/// 仅改写已存在 `model_provider` 且 != target 的 `session_meta` 行（保守：不新增字段，
/// 与本项目现有迁移行为一致；行级 provider 归属由 state DB 兜底）。
fn rewrite_session_meta_line_to_target(line: &str, target: &str) -> Option<String> {
    if !line.contains("\"session_meta\"") || !line.contains("\"model_provider\"") {
        return None;
    }
    let mut value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get_mut("payload")?.as_object_mut()?;
    let current = payload.get("model_provider")?.as_str()?;
    if current == target {
        return None;
    }
    payload.insert(
        "model_provider".to_string(),
        Value::String(target.to_string()),
    );
    serde_json::to_string(&value).ok()
}

/// user-event 检测，对齐参考项目 `recordHasUserEvent`。
fn record_has_user_event(value: &Value) -> bool {
    if value.get("type").and_then(Value::as_str) == Some("event_msg")
        && value
            .get("payload")
            .and_then(|p| p.get("type"))
            .and_then(Value::as_str)
            == Some("user_message")
    {
        return true;
    }
    for key in ["payload", "item", "msg"] {
        if let Some(inner) = value.get(key) {
            if inner.get("type").and_then(Value::as_str) == Some("message")
                && inner.get("role").and_then(Value::as_str) == Some("user")
            {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// state_5.sqlite 更新
// ---------------------------------------------------------------------------

struct StateUpdateStats {
    provider_rows: usize,
    user_event_rows: usize,
    cwd_rows: usize,
}

fn update_state_db(
    db_path: &Path,
    codex_dir: &Path,
    target: &str,
    thread_cwd_by_id: &BTreeMap<String, String>,
    user_event_thread_ids: &HashSet<String>,
    backup_root: &Path,
) -> Result<StateUpdateStats, AppError> {
    let mut conn = Connection::open(db_path)
        .map_err(|e| map_locked_db_error("打开 Codex state DB 失败", db_path, e))?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| AppError::Database(format!("设置 Codex state DB busy_timeout 失败: {e}")))?;

    if !Database::table_exists(&conn, "threads")?
        || !Database::has_column(&conn, "threads", "model_provider")?
    {
        return Ok(StateUpdateStats {
            provider_rows: 0,
            user_event_rows: 0,
            cwd_rows: 0,
        });
    }

    let has_user_event_col = Database::has_column(&conn, "threads", "has_user_event")?;
    let has_cwd_col = Database::has_column(&conn, "threads", "cwd")?;

    // 先备份再改。
    backup_codex_state_db(db_path, codex_dir, backup_root, &conn)?;

    let tx = conn
        .transaction()
        .map_err(|e| AppError::Database(format!("开启 Codex state DB 事务失败: {e}")))?;

    let provider_rows = tx
        .execute(
            "UPDATE threads SET model_provider = ?1 WHERE COALESCE(model_provider, '') <> ?1",
            params![target],
        )
        .map_err(|e| map_locked_db_error("更新 Codex threads provider 失败", db_path, e))?;

    let mut user_event_rows = 0usize;
    if has_user_event_col && !user_event_thread_ids.is_empty() {
        let mut stmt = tx
            .prepare(
                "UPDATE threads SET has_user_event = 1 \
                 WHERE id = ?1 AND COALESCE(has_user_event, 0) <> 1",
            )
            .map_err(|e| AppError::Database(format!("准备 has_user_event 更新失败: {e}")))?;
        for id in user_event_thread_ids {
            user_event_rows += stmt
                .execute(params![id])
                .map_err(|e| AppError::Database(format!("更新 has_user_event 失败: {e}")))?;
        }
    }

    let mut cwd_rows = 0usize;
    if has_cwd_col && !thread_cwd_by_id.is_empty() {
        let mut stmt = tx
            .prepare("UPDATE threads SET cwd = ?1 WHERE id = ?2 AND COALESCE(cwd, '') <> ?1")
            .map_err(|e| AppError::Database(format!("准备 cwd 更新失败: {e}")))?;
        for (id, cwd) in thread_cwd_by_id {
            if cwd.trim().is_empty() {
                continue;
            }
            cwd_rows += stmt
                .execute(params![cwd, id])
                .map_err(|e| AppError::Database(format!("更新 cwd 失败: {e}")))?;
        }
    }

    tx.commit()
        .map_err(|e| AppError::Database(format!("提交 Codex state DB 事务失败: {e}")))?;

    Ok(StateUpdateStats {
        provider_rows,
        user_event_rows,
        cwd_rows,
    })
}

fn map_locked_db_error(context: &str, db_path: &Path, err: rusqlite::Error) -> AppError {
    let msg = err.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("locked") || lower.contains("busy") {
        AppError::Database(format!(
            "{context}（{}）：数据库被占用，请关闭 Codex / Codex 应用后重试",
            db_path.display()
        ))
    } else if lower.contains("malformed")
        || lower.contains("not a database")
        || lower.contains("corrupt")
    {
        AppError::Database(format!(
            "{context}（{}）：数据库损坏，请先备份/修复后重试",
            db_path.display()
        ))
    } else {
        AppError::Database(format!("{context}（{}）: {msg}", db_path.display()))
    }
}

// ---------------------------------------------------------------------------
// 工作区缓存（.codex-global-state.json）同步
// ---------------------------------------------------------------------------

struct WorkspaceRootsOutcome {
    present: bool,
    updated: bool,
}

#[derive(Clone)]
struct CwdStat {
    cwd: String,
    normalized_cwd: String,
    count: i64,
    updated_at_ms: i64,
}

fn sync_workspace_roots(
    codex_dir: &Path,
    state_db_path: &Path,
    backup_root: &Path,
    skipped: &mut Vec<String>,
) -> Result<WorkspaceRootsOutcome, AppError> {
    let file_path = codex_dir.join(GLOBAL_STATE_FILE_BASENAME);
    let backup_sibling = codex_dir.join(GLOBAL_STATE_BACKUP_FILE_BASENAME);

    let original_text = match fs::read_to_string(&file_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceRootsOutcome {
                present: false,
                updated: false,
            });
        }
        Err(err) => return Err(AppError::io(&file_path, err)),
    };

    let mut state: Value = match serde_json::from_str(&original_text) {
        Ok(value @ Value::Object(_)) => value,
        Ok(_) => {
            skipped.push("workspace-roots: 顶层不是 JSON 对象，已跳过".to_string());
            return Ok(WorkspaceRootsOutcome {
                present: true,
                updated: false,
            });
        }
        Err(err) => {
            skipped.push(format!("workspace-roots: JSON 解析失败（{err}），已跳过"));
            return Ok(WorkspaceRootsOutcome {
                present: true,
                updated: false,
            });
        }
    };

    let stats = read_thread_cwd_stats(state_db_path).unwrap_or_default();

    let existing_saved = to_path_array(state.get("electron-saved-workspace-roots"));
    let existing_order = to_path_array(state.get("project-order"));
    let existing_active = to_path_array(state.get("active-workspace-roots"));

    // electron-saved-workspace-roots：project-order(若有) + saved + active，解析→去重。
    let saved_source: Vec<String> = if existing_order.is_empty() {
        [existing_saved.clone(), existing_active.clone()].concat()
    } else {
        [
            existing_order.clone(),
            existing_saved.clone(),
            existing_active.clone(),
        ]
        .concat()
    };
    let next_saved = dedupe_paths(
        &saved_source
            .iter()
            .map(|p| resolve_stored_path(p, &stats))
            .collect::<Vec<_>>(),
    );

    // project-order：project-order(若有) + saved，解析→去重；否则种子自 next_saved。
    let next_order = if existing_order.is_empty() {
        dedupe_paths(&next_saved)
    } else {
        dedupe_paths(
            &[existing_order.clone(), existing_saved.clone()]
                .concat()
                .iter()
                .map(|p| resolve_stored_path(p, &stats))
                .collect::<Vec<_>>(),
        )
    };

    let next_active = dedupe_paths(
        &existing_active
            .iter()
            .map(|p| resolve_stored_path(p, &stats))
            .collect::<Vec<_>>(),
    );

    let original_active_value = state.get("active-workspace-roots").cloned();
    let next_active_value = match &original_active_value {
        Some(Value::Array(_)) => Value::Array(string_array(&next_active)),
        _ => match next_active.first() {
            Some(first) => Value::String(first.clone()),
            None => original_active_value.clone().unwrap_or(Value::Null),
        },
    };

    let original_labels = state.get("electron-workspace-root-labels").cloned();
    let next_labels = original_labels
        .as_ref()
        .map(|value| copy_resolved_object_keys(value, &stats));

    let original_open_targets = state.get("open-in-target-preferences").cloned();
    let next_open_targets = match &original_open_targets {
        Some(Value::Object(map)) => {
            let mut new_map = map.clone();
            if let Some(per_path) = map.get("perPath") {
                new_map.insert(
                    "perPath".to_string(),
                    copy_resolved_object_keys(per_path, &stats),
                );
            }
            Some(Value::Object(new_map))
        }
        other => other.clone(),
    };

    let saved_changed = existing_saved != next_saved;
    let order_changed = existing_order != next_order;
    let active_changed = original_active_value.clone().unwrap_or(Value::Null) != next_active_value;
    let labels_changed =
        original_labels.clone().unwrap_or(Value::Null) != next_labels.clone().unwrap_or(Value::Null);
    let open_changed = original_open_targets.clone().unwrap_or(Value::Null)
        != next_open_targets.clone().unwrap_or(Value::Null);
    let backup_missing = !backup_sibling.exists();
    let updated =
        saved_changed || order_changed || active_changed || labels_changed || open_changed || backup_missing;

    {
        let obj = state
            .as_object_mut()
            .expect("state 已校验为 JSON 对象");
        obj.insert(
            "electron-saved-workspace-roots".to_string(),
            Value::Array(string_array(&next_saved)),
        );
        obj.insert(
            "project-order".to_string(),
            Value::Array(string_array(&next_order)),
        );
        obj.insert("active-workspace-roots".to_string(), next_active_value);
        if let Some(labels) = next_labels {
            obj.insert("electron-workspace-root-labels".to_string(), labels);
        }
        if let Some(open) = next_open_targets {
            obj.insert("open-in-target-preferences".to_string(), open);
        }
    }

    if updated {
        // 先把原始两份备份进时间戳目录，再原子写出新内容到主文件 + 同级 .bak。
        backup_global_state(codex_dir, backup_root)?;
        let mut next_text =
            serde_json::to_string_pretty(&state).map_err(|e| AppError::JsonSerialize { source: e })?;
        next_text.push('\n');
        atomic_write(&file_path, next_text.as_bytes())?;
        atomic_write(&backup_sibling, next_text.as_bytes())?;
    }

    Ok(WorkspaceRootsOutcome {
        present: true,
        updated,
    })
}

fn backup_global_state(codex_dir: &Path, backup_root: &Path) -> Result<(), AppError> {
    for basename in [GLOBAL_STATE_FILE_BASENAME, GLOBAL_STATE_BACKUP_FILE_BASENAME] {
        let src = codex_dir.join(basename);
        if !src.exists() {
            continue;
        }
        let dest = backup_root.join("global-state").join(basename);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        copy_file(&src, &dest)?;
    }
    Ok(())
}

fn read_thread_cwd_stats(db_path: &Path) -> Result<Vec<CwdStat>, AppError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::Database(format!("打开 Codex state DB 失败: {e}")))?;
    let _ = conn.busy_timeout(Duration::from_secs(5));

    if !Database::table_exists(&conn, "threads")? || !Database::has_column(&conn, "threads", "cwd")? {
        return Ok(Vec::new());
    }
    let has_updated_at = Database::has_column(&conn, "threads", "updated_at")?;
    let sql = if has_updated_at {
        "SELECT cwd, COUNT(*) AS cnt, COALESCE(MAX(updated_at), 0) AS upd \
         FROM threads WHERE cwd IS NOT NULL AND cwd <> '' GROUP BY cwd"
    } else {
        "SELECT cwd, COUNT(*) AS cnt, 0 AS upd \
         FROM threads WHERE cwd IS NOT NULL AND cwd <> '' GROUP BY cwd"
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::Database(format!("准备 cwd 统计失败: {e}")))?;
    let rows = stmt
        .query_map([], |row| {
            let cwd: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            // updated_at 可能是文本时间戳，取 i64 失败则退化为 0（仅影响并列时的次序）。
            let updated_at_ms: i64 = row.get(2).unwrap_or(0);
            Ok((cwd, count, updated_at_ms))
        })
        .map_err(|e| AppError::Database(format!("查询 cwd 统计失败: {e}")))?;

    let mut stats = Vec::new();
    for row in rows {
        let (cwd, count, updated_at_ms) =
            row.map_err(|e| AppError::Database(format!("读取 cwd 统计失败: {e}")))?;
        if let Some(normalized_cwd) = normalize_comparable_path(&cwd) {
            stats.push(CwdStat {
                cwd,
                normalized_cwd,
                count,
                updated_at_ms,
            });
        }
    }
    Ok(stats)
}

// ---------------------------------------------------------------------------
// 路径 / 数组工具（平台感知）
// ---------------------------------------------------------------------------

fn string_array(values: &[String]) -> Vec<Value> {
    values.iter().cloned().map(Value::String).collect()
}

/// 数组 → 非空（trim 后）字符串项；单个非空字符串 → 单元素数组；否则空。
fn to_path_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// 去重：按归一化比较键去重，保留原始值（保留首次出现顺序）。
fn dedupe_paths(paths: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for p in paths {
        if let Some(comparable) = normalize_comparable_path(p) {
            if seen.insert(comparable) {
                out.push(p.clone());
            }
        }
    }
    out
}

/// 把存储路径解析到"最匹配的真实 cwd"（按 count→最近→字典序），无匹配则仅做展示归一化。
fn resolve_stored_path(value: &str, stats: &[CwdStat]) -> String {
    let comparable = match normalize_comparable_path(value) {
        Some(c) => c,
        None => return value.to_string(),
    };
    let mut matches: Vec<&CwdStat> = stats
        .iter()
        .filter(|e| e.normalized_cwd == comparable)
        .collect();
    if matches.is_empty() {
        return to_desktop_path(value);
    }
    matches.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(b.updated_at_ms.cmp(&a.updated_at_ms))
            .then(a.cwd.cmp(&b.cwd))
    });
    to_desktop_path(&matches[0].cwd)
}

/// 重映射对象的键（路径），冲突时除非解析后等于原键否则保留已有项。非对象原样返回。
fn copy_resolved_object_keys(input: &Value, stats: &[CwdStat]) -> Value {
    match input {
        Value::Object(map) => {
            let mut result = Map::new();
            for (key, val) in map {
                let resolved = resolve_stored_path(key, stats);
                if !result.contains_key(&resolved) || resolved == *key {
                    result.insert(resolved, val.clone());
                }
            }
            Value::Object(result)
        }
        other => other.clone(),
    }
}

/// 展示用路径归一化（对齐参考项目 `toDesktopWorkspacePath`）。
/// 仅 Windows 处理 `\\?\` 扩展前缀 / 盘符 / 斜杠；POSIX 原样返回，避免破坏路径。
fn to_desktop_path(value: &str) -> String {
    if !cfg!(windows) {
        return value.to_string();
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.to_string();
    }
    if let Some(rest) = strip_prefix_ci(trimmed, "\\\\?\\UNC\\") {
        return format!("\\\\{}", rest.replace('/', "\\"));
    }
    if let Some(after) = strip_prefix_ci(trimmed, "\\\\?\\") {
        let bytes = after.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            let drive = &after[..2];
            let rest = after[2..].trim_start_matches(['\\', '/']);
            if rest.is_empty() {
                return format!("{drive}\\");
            }
            return format!("{drive}\\{}", rest.replace('/', "\\"));
        }
        return after.replace('/', "\\");
    }
    value.to_string()
}

/// 比较用路径归一化（对齐参考项目 `normalizeComparablePath`，平台感知）。
fn normalize_comparable_path(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if cfg!(windows) {
        let mut s = if let Some(rest) = strip_prefix_ci(trimmed, "\\\\?\\UNC\\") {
            format!("\\\\{rest}")
        } else if let Some(after) = strip_prefix_ci(trimmed, "\\\\?\\") {
            after.to_string()
        } else {
            trimmed.to_string()
        };
        s = s.replace('/', "\\");
        while s.len() > 1 && s.ends_with('\\') {
            s.pop();
        }
        let bytes = s.as_bytes();
        if s.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            s.push('\\');
        }
        Some(s.to_lowercase())
    } else {
        let mut s = trimmed.to_string();
        while s.len() > 1 && s.ends_with('/') {
            s.pop();
        }
        if cfg!(target_os = "macos") {
            s = s.to_lowercase();
        }
        Some(s)
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seed_threads(db_path: &Path, ddl: &str, inserts: &str) {
        let conn = Connection::open(db_path).expect("open db");
        conn.execute_batch(ddl).expect("ddl");
        conn.execute_batch(inserts).expect("insert");
    }

    #[test]
    fn current_model_provider_reads_or_falls_back() {
        assert_eq!(current_model_provider(""), "openai");
        assert_eq!(
            current_model_provider("model_provider = \"custom\"\n"),
            "custom"
        );
        assert_eq!(
            current_model_provider("model_provider = \"   \"\n"),
            "openai"
        );
    }

    #[test]
    fn rewrites_session_meta_when_provider_differs_and_skips_when_equal() {
        let dir = tempdir().expect("tempdir");
        let codex_dir = dir.path().join(".codex");
        let backup_root = dir.path().join("backup");
        let session_dir = codex_dir.join("sessions/2026/06/01");
        fs::create_dir_all(&session_dir).expect("mkdir");

        // model_provider 与 target 不同 → 改写。
        let p1 = session_dir.join("rollout-a.jsonl");
        fs::write(
            &p1,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"t1\",\"cwd\":\"/Users/me/proj\",\"model_provider\":\"openai\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"hi\"}}\n",
            ),
        )
        .expect("write p1");

        let info = process_session_file(&p1, &codex_dir, "custom", &backup_root).expect("process");
        assert!(info.rewritten);
        assert_eq!(info.id.as_deref(), Some("t1"));
        assert_eq!(info.cwd.as_deref(), Some("/Users/me/proj")); // POSIX 路径未被反斜杠破坏
        assert!(info.has_user_event);
        let text = fs::read_to_string(&p1).expect("read p1");
        assert!(text.contains("\"model_provider\":\"custom\""));
        assert!(!text.contains("\"model_provider\":\"openai\""));
        assert!(backup_root
            .join("jsonl/sessions/2026/06/01/rollout-a.jsonl")
            .exists());

        // 已等于 target → 不改写、无备份。
        let p2 = session_dir.join("rollout-b.jsonl");
        fs::write(
            &p2,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"t2\",\"cwd\":\"/x\",\"model_provider\":\"custom\"}}\n",
        )
        .expect("write p2");
        let info2 = process_session_file(&p2, &codex_dir, "custom", &backup_root).expect("process2");
        assert!(!info2.rewritten);
        assert_eq!(info2.id.as_deref(), Some("t2"));
    }

    #[test]
    fn updates_state_db_provider_and_repairs_columns() {
        let dir = tempdir().expect("tempdir");
        let codex_dir = dir.path().join(".codex");
        let backup_root = dir.path().join("backup");
        fs::create_dir_all(&codex_dir).expect("mkdir");
        let db_path = codex_dir.join(CODEX_STATE_DB_FILENAME);
        seed_threads(
            &db_path,
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, has_user_event INTEGER, cwd TEXT);",
            "INSERT INTO threads (id, model_provider, has_user_event, cwd) VALUES
                ('t1','openai',0,'/old'),
                ('t2','custom',1,'/Users/me/proj'),
                ('t3','deepseek',NULL,NULL);",
        );

        let mut cwds = BTreeMap::new();
        cwds.insert("t1".to_string(), "/Users/me/proj".to_string());
        let mut events = HashSet::new();
        events.insert("t1".to_string());

        let stats = update_state_db(&db_path, &codex_dir, "custom", &cwds, &events, &backup_root)
            .expect("update");
        assert_eq!(stats.provider_rows, 2); // t1, t3 changed; t2 already custom
        assert_eq!(stats.user_event_rows, 1); // t1 0->1
        assert_eq!(stats.cwd_rows, 1); // t1 /old->/Users/me/proj

        let conn = Connection::open(&db_path).expect("reopen");
        let all_custom: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE model_provider = 'custom'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(all_custom, 3);
        let t1_event: i64 = conn
            .query_row("SELECT has_user_event FROM threads WHERE id='t1'", [], |r| {
                r.get(0)
            })
            .expect("event");
        assert_eq!(t1_event, 1);
        assert!(backup_root.join("state").join(CODEX_STATE_DB_FILENAME).exists());
    }

    #[test]
    fn state_db_update_skips_missing_optional_columns() {
        let dir = tempdir().expect("tempdir");
        let codex_dir = dir.path().join(".codex");
        let backup_root = dir.path().join("backup");
        fs::create_dir_all(&codex_dir).expect("mkdir");
        let db_path = codex_dir.join(CODEX_STATE_DB_FILENAME);
        seed_threads(
            &db_path,
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT);",
            "INSERT INTO threads (id, model_provider) VALUES ('t1','openai');",
        );

        let mut cwds = BTreeMap::new();
        cwds.insert("t1".to_string(), "/p".to_string());
        let mut events = HashSet::new();
        events.insert("t1".to_string());

        let stats = update_state_db(&db_path, &codex_dir, "custom", &cwds, &events, &backup_root)
            .expect("update");
        assert_eq!(stats.provider_rows, 1);
        assert_eq!(stats.user_event_rows, 0); // no has_user_event column
        assert_eq!(stats.cwd_rows, 0); // no cwd column
    }

    #[test]
    fn workspace_roots_absent_is_not_an_error() {
        let dir = tempdir().expect("tempdir");
        let codex_dir = dir.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("mkdir");
        let db_path = codex_dir.join(CODEX_STATE_DB_FILENAME);
        let mut skipped = Vec::new();
        let out = sync_workspace_roots(&codex_dir, &db_path, &dir.path().join("backup"), &mut skipped)
            .expect("sync ws");
        assert!(!out.present);
        assert!(!out.updated);
    }

    #[test]
    fn workspace_roots_rebuilds_dedupes_and_writes_bak_without_mangling_posix() {
        let dir = tempdir().expect("tempdir");
        let codex_dir = dir.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("mkdir");
        let db_path = codex_dir.join(CODEX_STATE_DB_FILENAME);
        seed_threads(
            &db_path,
            "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT);",
            "INSERT INTO threads (id, cwd) VALUES ('t1','/Users/me/proj'),('t2','/Users/me/proj');",
        );

        let global = codex_dir.join(GLOBAL_STATE_FILE_BASENAME);
        fs::write(
            &global,
            "{\"electron-saved-workspace-roots\":[\"/Users/me/proj\",\"/Users/me/proj\"],\"project-order\":[],\"active-workspace-roots\":[\"/Users/me/proj\"]}",
        )
        .expect("write global");

        let mut skipped = Vec::new();
        let out =
            sync_workspace_roots(&codex_dir, &db_path, &dir.path().join("backup"), &mut skipped)
                .expect("sync ws");
        assert!(out.present);
        assert!(out.updated);

        let written = fs::read_to_string(&global).expect("read global");
        assert!(!written.contains('\\')); // POSIX 路径未被反斜杠破坏
        assert!(codex_dir.join(GLOBAL_STATE_BACKUP_FILE_BASENAME).exists());

        let parsed: Value = serde_json::from_str(&written).expect("parse");
        let saved = parsed
            .get("electron-saved-workspace-roots")
            .and_then(Value::as_array)
            .expect("saved array");
        assert_eq!(saved.len(), 1); // 去重后仅 1 项
        assert_eq!(saved[0].as_str(), Some("/Users/me/proj"));
    }

    #[test]
    fn posix_path_helpers_are_identity_safe() {
        // 非 Windows 下不应把 / 变成 \
        if !cfg!(windows) {
            assert_eq!(to_desktop_path("/Users/me/proj"), "/Users/me/proj");
            assert_eq!(
                normalize_comparable_path("/Users/me/proj/").as_deref(),
                Some(if cfg!(target_os = "macos") {
                    "/users/me/proj"
                } else {
                    "/Users/me/proj"
                })
            );
        }
    }
}
