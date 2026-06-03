# API Switch

> Claude Code · Codex · Gemini CLI 등 여러 AI 코딩 도구의 API 공급자 설정을 한곳에서 관리하고 **원클릭으로 전환**하는 데스크톱 앱.
>
> 하나의 데스크톱 앱으로 여러 AI 코딩 CLI의 API 공급자 설정을 관리하고 원클릭 전환 / A desktop app to manage and one-click switch API provider configs across multiple AI coding CLIs.

**언어 / 语言 / Language:** [한국어](#한국어) · [中文](#中文) · [English](#english)

macOS (Apple Silicon + Intel universal) · Windows · Linux · 기반: Tauri 2 + React

---

## 한국어

### 소개
API Switch는 Claude Code, OpenAI Codex, Gemini CLI 등 여러 AI 코딩 도구가 사용하는 API 공급자(엔드포인트 · API Key · 모델) 설정을 한 화면에서 관리하고, 클릭 한 번으로 전환할 수 있는 데스크톱 앱입니다. 공식 공급자에 더해 직접 구성한 게이트웨이도 추가할 수 있습니다.

### 주요 기능
- **원클릭 공급자 전환** — 각 도구의 설정 파일을 자동으로 다시 작성해 즉시 적용
- **여러 도구 지원** — Claude Code, Claude Desktop, OpenAI Codex, Gemini CLI, Hermes, OpenCode, OpenClaw
- **공식 공급자 프리셋** — 자주 쓰는 공식 공급자를 빠르게 추가
- **통합 공급자(Universal)** — 하나의 설정을 Claude · Codex · Gemini에 동시 동기화
- **프록시 / 라우팅** — 로컬 프록시, 라우팅 규칙, 자동 장애 조치(failover), 모델 테스트
- **사용량 통계** — 공급자별 사용량 조회
- **확장 관리** — MCP 서버, 프롬프트, 스킬, 세션, 워크스페이스 파일
- **Codex 세션 동기화** — 공급자 전환 후 사라진 이전 세션을 현재 공급자로 다시 정렬
- **다국어** — 한국어(기본) · 中文 · English · 日本語

### 설치 (macOS)
1. `API Switch_x.y.z_universal.dmg`를 열고 앱을 `응용 프로그램`으로 드래그합니다.
2. 처음 실행 시 서명되지 않은 앱이라 차단될 수 있습니다 → **우클릭 → 열기** 한 번이면 됩니다.
   - 그래도 "손상됨"이라고 나오면: `xattr -dr com.apple.quarantine "/Applications/API Switch.app"`

### 간단 사용법
1. 앱을 열고 상단에서 도구(Claude / Codex / Gemini …)를 선택합니다.
2. **현재 구성 가져오기**로 기존 설정을 default 공급자에 안전하게 저장하거나, **공급자 추가**로 새 공급자를 만듭니다.
3. 공급자 카드를 클릭하면 즉시 전환됩니다.
4. 터미널에서 해당 도구를 다시 실행하면 새 설정이 적용됩니다.

### 설정 파일 위치
사용자 데이터는 홈 디렉터리의 앱 설정 폴더에 저장됩니다.

---

## 中文

### 简介
API Switch 是一个桌面应用，把 Claude Code、OpenAI Codex、Gemini CLI 等多种 AI 编程工具的 API 供应商配置（接口地址 · API Key · 模型）集中管理，并一键切换。除官方供应商外，也可添加自定义网关。

### 功能特性
- **一键切换供应商** —— 自动改写各工具的配置文件，立即生效
- **多工具支持** —— Claude Code、Claude Desktop、OpenAI Codex、Gemini CLI、Hermes、OpenCode、OpenClaw
- **官方供应商预设** —— 快速添加常用官方供应商
- **统一供应商（Universal）** —— 一份配置同时同步到 Claude · Codex · Gemini
- **代理 / 路由** —— 本地代理、路由规则、自动故障转移、模型测试
- **用量统计** —— 按供应商查询使用量
- **扩展管理** —— MCP 服务、提示词、技能、会话、工作区文件
- **Codex 会话同步** —— 切换供应商后把"消失"的历史会话重新对齐到当前供应商
- **多语言** —— 한국어（默认）· 中文 · English · 日本語

### 安装 (macOS)
1. 打开 `API Switch_x.y.z_universal.dmg`，把应用拖到 `应用程序`。
2. 首次打开因未签名可能被拦截 → **右键 → 打开** 一次即可。
   - 若提示"已损坏"：`xattr -dr com.apple.quarantine "/Applications/API Switch.app"`

### 简单使用
1. 打开应用，在顶部选择工具（Claude / Codex / Gemini …）。
2. 用 **导入当前配置** 把现有设置安全保存为 default 供应商，或用 **添加供应商** 新建。
3. 点击供应商卡片即可立即切换。
4. 在终端重新运行对应工具，新配置生效。

### 配置文件位置
用户数据保存在主目录下的应用配置文件夹中。

---

## English

### Introduction
API Switch is a desktop app that centralizes the API provider settings (endpoint · API key · model) for multiple AI coding tools — Claude Code, OpenAI Codex, Gemini CLI and more — and lets you switch between them with one click. Beyond official providers, you can add your own custom gateways.

### Features
- **One-click provider switching** — rewrites each tool's config file automatically, applied instantly
- **Multiple tools** — Claude Code, Claude Desktop, OpenAI Codex, Gemini CLI, Hermes, OpenCode, OpenClaw
- **Official provider presets** — quickly add common official providers
- **Universal provider** — one config synced to Claude · Codex · Gemini at once
- **Proxy / routing** — local proxy, routing rules, automatic failover, model testing
- **Usage statistics** — per-provider usage query
- **Extensions** — MCP servers, prompts, skills, sessions, workspace files
- **Codex session sync** — realign past sessions that "disappeared" after a provider switch to the current provider
- **Multilingual** — Korean (default) · 中文 · English · 日本語

### Install (macOS)
1. Open `API Switch_x.y.z_universal.dmg` and drag the app into `Applications`.
2. On first launch the unsigned app may be blocked → just **right-click → Open** once.
   - If it says "damaged": `xattr -dr com.apple.quarantine "/Applications/API Switch.app"`

### Quick start
1. Open the app and pick a tool (Claude / Codex / Gemini …) at the top.
2. Use **Import current config** to safely save your existing setup as the default provider, or **Add provider** to create a new one.
3. Click a provider card to switch instantly.
4. Restart that tool in your terminal — the new config takes effect.

### Config location
User data is stored in the app config folder under your home directory.

---

## Build from source / 从源码构建 / 소스 빌드

```bash
pnpm install
pnpm tauri dev                                  # 개발 / 开发 / dev
pnpm tauri build                                # 현재 아키텍처 / 当前架构 / current arch
pnpm tauri build --target universal-apple-darwin  # macOS universal (Intel + ARM)
```

Requirements: Node.js + pnpm, Rust toolchain, and the Tauri prerequisites for your OS.

## License / 라이선스 / 许可

[MIT](LICENSE). API Switch is a fork; the original MIT copyright notice is retained.
