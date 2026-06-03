import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import zh from "./locales/zh.json";

type Language = "ko" | "zh" | "en" | "ja";

const DEFAULT_LANGUAGE: Language = "ko";

const getInitialLanguage = (): Language => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem("language");
      if (
        stored === "ko" ||
        stored === "zh" ||
        stored === "en" ||
        stored === "ja"
      ) {
        return stored;
      }
    } catch (error) {
      console.warn("[i18n] Failed to read stored language preference", error);
    }
  }

  // 默认语言固定为韩语（面向韩国用户）。不再根据浏览器语言自动探测；
  // 简体中文 / 英文 / 日文可在设置中手动切换，并会被持久化记住。
  return DEFAULT_LANGUAGE;
};

const resources = {
  ko: {
    translation: ko,
  },
  en: {
    translation: en,
  },
  ja: {
    translation: ja,
  },
  zh: {
    translation: zh,
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: getInitialLanguage(), // 优先使用已保存的语言，否则默认韩语
  fallbackLng: ["ko", "en"], // 缺失的 key 先回退韩语，再回退英文

  interpolation: {
    escapeValue: false, // React 已经默认转义
  },

  // 开发模式下显示调试信息
  debug: false,
});

export default i18n;
