import { invoke } from "@tauri-apps/api/core";

/**
 * Codex 会话同步结果（对应 Rust `CodexSessionSyncOutcome`，camelCase）。
 */
export interface CodexSessionSyncResult {
  /** 同步目标 provider（来自 live config.toml，缺省 "openai"）。 */
  targetProvider: string;
  /** 被改写 model_provider 的会话 JSONL 文件数。 */
  rewroteJsonlFiles: number;
  /** state_5.sqlite threads.model_provider 被更新的行数。 */
  updatedStateRows: number;
  /** threads.has_user_event 被修复的行数。 */
  updatedUserEventRows: number;
  /** threads.cwd 被修复的行数。 */
  updatedCwdRows: number;
  /** 是否存在 .codex-global-state.json。 */
  workspaceRootsPresent: boolean;
  /** 工作区缓存是否被写入。 */
  workspaceRootsUpdated: boolean;
  /** 被跳过的项（被占用的会话文件 / 无法解析的全局状态等）。 */
  skipped: string[];
}

export const codexApi = {
  /**
   * 把 Codex 会话 / state_5.sqlite / 工作区缓存同步到当前 provider，
   * 让切换 provider 后"消失"的历史会话重新可见。
   */
  async syncSessions(): Promise<CodexSessionSyncResult> {
    return await invoke("sync_codex_sessions");
  },
};
