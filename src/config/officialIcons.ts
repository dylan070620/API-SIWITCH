/**
 * 「选择图标」选择器中可见的官方图标白名单（按图标 name 精确匹配）。
 *
 * 设计说明：
 * - 仅控制图标选择器（IconPicker）的可见性，**不删除任何 SVG/iconUrls/metadata**，
 *   因此 cn_official 模型厂等预设仍能正常渲染其图标（如 bailian/huoshan/byteplus/doubao）。
 * - 白名单只保留官方品牌：官方三家 + 首方模型厂 + OpenRouter + 官方云 + 工具/应用/协议。
 * - 第三方中转站/转售商、非官方聚合（SiliconFlow/Novita/ModelScope/AiHubMix…）、
 *   非官方云（火山/BytePlus/UCloud）不在此列表，因而从选择器隐藏。
 */
export const OFFICIAL_ICONS: ReadonlySet<string> = new Set([
  // 官方三家
  "anthropic",
  "claude",
  "openai",
  "gemini",
  "google",
  "googlecloud",
  "gemma",
  "palm",
  // 首方模型厂
  "deepseek",
  "zhipu",
  "chatglm",
  "kimi",
  "moonshot",
  "minimax",
  "stepfun",
  "qwen",
  "alibaba",
  "baidu",
  "wenxin",
  "tencent",
  "hunyuan",
  "doubao",
  "bytedance",
  "xiaomimimo",
  "longcat",
  "catcoder",
  "yi",
  "zeroone",
  "meta",
  "mistral",
  "cohere",
  "grok",
  "xai",
  "perplexity",
  "nvidia",
  "huggingface",
  "stability",
  "midjourney",
  // 用户保留：OpenRouter + Nous Research(hermes)
  "openrouter",
  "hermes",
  // 官方云
  "aws",
  "azure",
  "cloudflare",
  "huawei",
  // 工具 / 应用 / 协议 / 兜底
  "github",
  "githubcopilot",
  "copilot",
  "ollama",
  "vercel",
  "notion",
  "mcp",
  "opencode",
  "openclaw",
  "generic",
]);

/**
 * 判断某个图标是否为官方图标（应在「选择图标」选择器中显示）。
 */
export function isOfficialIcon(name: string): boolean {
  return OFFICIAL_ICONS.has(name.toLowerCase());
}
