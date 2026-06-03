/**
 * 各应用「添加供应商」弹窗中可见的预设白名单（按预设 name 精确匹配）。
 *
 * 设计说明：
 * - 预设数组本身保持完整（provider 分类识别、模板变量、deeplink 导入、预设单测等都依赖它），
 *   这里只控制 UI 列表的可见性，不删除任何预设条目。
 * - 过滤必须在 `.map((preset, index) => ({ id: `<app>-${index}` }))` 之后进行，
 *   以保证可见项的 id 仍等于其在原始数组中的下标
 *   （AddProviderDialog 会按裸下标反查 endpointCandidates）。
 * - 未列入下表的应用一律不过滤（显示全部）。
 * - opencode / openclaw 没有官方/原生预设，因此仅保留「自定义配置」。
 */
export const VISIBLE_PRESET_NAMES: Record<string, ReadonlySet<string>> = {
  claude: new Set([
    "Claude Official",
    "Gemini Native",
    "GitHub Copilot",
    "Codex",
    "OpenRouter",
  ]),
  "claude-desktop": new Set(["Claude Desktop Official"]),
  codex: new Set(["OpenAI Official", "Azure OpenAI"]),
  gemini: new Set(["Google Official"]),
  hermes: new Set(["Nous Research"]),
  opencode: new Set<string>([]),
  openclaw: new Set<string>([]),
};

/**
 * 判断某个预设是否应在指定应用的添加供应商弹窗中显示。
 * 未在白名单表中的应用返回 true（不过滤）。
 */
export function isPresetVisible(appId: string, presetName: string): boolean {
  const allow = VISIBLE_PRESET_NAMES[appId];
  if (!allow) return true;
  return allow.has(presetName);
}
