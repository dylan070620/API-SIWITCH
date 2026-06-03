/**
 * Codex 会话同步 Hook。
 *
 * 触发后端 `sync_codex_sessions`，把会话 / state_5.sqlite / 工作区缓存的
 * model_provider 统一同步到当前 provider，让切换后"消失"的历史会话重新可见。
 * 同步中通过 `isSyncing` 暴露 loading 态以禁用按钮、避免重复点击；
 * 成功 / 失败分别弹 toast。
 */

import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { codexApi, type CodexSessionSyncResult } from "@/lib/api";
import { extractErrorMessage } from "@/utils/errorUtils";

export function useCodexSync() {
  const { t } = useTranslation();

  const mutation = useMutation<CodexSessionSyncResult, Error>({
    mutationFn: () => codexApi.syncSessions(),
    onSuccess: (result) => {
      toast.success(
        t("codexSync.success", {
          files: result.rewroteJsonlFiles,
          threads: result.updatedStateRows,
          defaultValue:
            "会话同步完成（{{files}} 个文件 / {{threads}} 条记录）",
        }),
      );
    },
    onError: (error) => {
      const detail =
        extractErrorMessage(error) ||
        t("common.unknown", { defaultValue: "未知错误" });
      toast.error(
        t("codexSync.failed", {
          error: detail,
          defaultValue: "同步会话失败：{{error}}",
        }),
      );
    },
  });

  return {
    /** 触发一次同步（忽略重复触发，loading 期间按钮应禁用）。 */
    sync: () => mutation.mutate(),
    /** 是否正在同步。 */
    isSyncing: mutation.isPending,
  };
}
