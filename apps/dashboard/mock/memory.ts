import type { MemoryView } from "../src/api/types.ts";

export interface MockMemoryData {
  user: { text: string; updated_at: string | null };
  memories: MemoryView[];
}

function ago(minutes: number): string {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

const DAY = 60 * 24;

const userText = `# 关于我

- 称呼：安东（Anthod），独立开发者，主要在 Linux（niri / Wayland）上工作。
- 语言：日常交流用中文；代码、提交信息和英文文档用英文。
- 时区：Asia/Shanghai。

## 偏好

- 回复简洁，先给结论，再给必要细节。
- TypeScript 项目默认用 bun；Rust 用 stable 工具链。
- 设计类任务先出原型和截图，确认后再实现。
- 临时任务放在 \`~/Pontia/YYYY-MM-DD_主题\` 目录。

## 约定

- 涉及删除数据、推送或部署的操作先问我。
- 产品文案描述 Pontia 能做什么，少写“不支持什么”。
`;

type Seed = Omit<
  MemoryView,
  | "keywords"
  | "importance"
  | "archived"
  | "organized"
  | "created_by"
  | "updated_at"
  | "last_read_at"
  | "occurred_at"
  | "sources"
  | "workspace_id"
  | "session_id"
> &
  Partial<MemoryView> & { keywords: string[] };

function memory(seed: Seed): MemoryView {
  return {
    workspace_id: null,
    session_id: null,
    archived: false,
    organized: true,
    importance: "normal",
    occurred_at: null,
    sources: null,
    created_by: "bot",
    updated_at: seed.created_at,
    last_read_at: null,
    ...seed,
  };
}

export function createSampleMemoryData(): MockMemoryData {
  const memories: MemoryView[] = [
    memory({
      id: "mem_019a7c1e-01",
      kind: "decision",
      content:
        "Dashboard 的 Settings 新栏目统一放在左侧设置导航中，手机端在顶栏横向切换。新增栏目沿用 Agent Profiles 的“列表 + 详情”布局，窄屏时详情覆盖列表并提供返回按钮。",
      keywords: ["dashboard", "settings", "布局", "mobile"],
      workspace_id: "wks_mock_pontia",
      session_id: "sess_mock_bot_main",
      importance: "high",
      sources:
        "2026-11-18 与用户在 Bot 对话中确认（消息 msg_0198…41）；参考 apps/dashboard/src/pages/AgentProfilesPage.svelte。",
      created_at: ago(5 * DAY),
      updated_at: ago(2 * DAY),
      last_read_at: ago(40),
    }),
    memory({
      id: "mem_019a7c1e-02",
      kind: "fact",
      content:
        "Pontia 仓库位于 ~/projects/pontia。Dashboard 是 SvelteKit 静态 SPA（apps/dashboard），后端 pontiad 由 Rust crates 组成，数据存在 SQLite。`just dev-dashboard` 可以用 mock API 单独启动 Dashboard。",
      keywords: ["pontia", "仓库", "dashboard", "just dev"],
      workspace_id: "wks_mock_pontia",
      session_id: "sess_mock_bot_main",
      importance: "high",
      created_at: ago(12 * DAY),
      updated_at: ago(12 * DAY),
      last_read_at: ago(3 * 60),
    }),
    memory({
      id: "mem_019a7c1e-03",
      kind: "lesson",
      content:
        "在 worktree 中跑 Dashboard 前要先 `bun install --cwd apps/dashboard`，否则 vite 会从主仓库的 node_modules 解析依赖，导致 Svelte 版本不一致的报错。",
      keywords: ["worktree", "bun install", "vite", "依赖"],
      workspace_id: "wks_mock_pontia",
      session_id: "sess_mock_helper_07",
      sources: "helper sess_mock_helper_07 的排查记录（2026-11-15）。",
      created_at: ago(8 * DAY),
      updated_at: ago(8 * DAY),
      last_read_at: ago(1 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-04",
      kind: "event",
      content:
        "发布 Pontia 0.8.5：支持 Bot 头像形状与颜色，修复 Codex 会话标题。用户确认发布说明后由 helper 打 tag 并推送。",
      keywords: ["release", "0.8.5", "发布"],
      workspace_id: "wks_mock_pontia",
      session_id: "sess_mock_bot_main",
      occurred_at: ago(9 * DAY),
      created_at: ago(9 * DAY),
      created_by: "dreaming",
      sources: "合并自两条 Bot 记忆（发布前确认、发布完成通知）。",
    }),
    memory({
      id: "mem_019a7c1e-05",
      kind: "observation",
      content: "用户评审 UI 时通常先看手机端截图，再看桌面端；深色模式问题经常在评审中被提出。",
      keywords: ["ui 评审", "截图", "深色模式", "mobile"],
      importance: "normal",
      organized: false,
      created_at: ago(1 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-06",
      kind: "decision",
      content:
        "个人网站改用 Astro 静态生成，部署到 Cloudflare Pages；博客文章继续用 Markdown 放在 content/posts。",
      keywords: ["website", "astro", "cloudflare pages", "部署"],
      workspace_id: "wks_mock_website",
      session_id: "sess_mock_bot_side",
      importance: "high",
      created_at: ago(20 * DAY),
      updated_at: ago(6 * DAY),
      last_read_at: ago(6 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-07",
      kind: "fact",
      content:
        "网站的 Cloudflare 账号用 GitHub 登录，项目名为 anthod-site；预览分支会自动生成 *.anthod-site.pages.dev 地址。",
      keywords: ["cloudflare", "anthod-site", "预览"],
      workspace_id: "wks_mock_website",
      created_at: ago(19 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-08",
      kind: "lesson",
      content:
        "截图脚本使用系统 Chromium（/usr/bin/chromium-browser），不要下载 Playwright 自带浏览器；本地 mock 页面可以用 headless 模式。",
      keywords: ["playwright", "chromium", "截图"],
      importance: "low",
      created_at: ago(15 * DAY),
      last_read_at: ago(2 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-09",
      kind: "event",
      content:
        "整理了 2026 年第 46 周的读书笔记，新增《Designing Data-Intensive Applications》第 5 章的复制策略摘要。",
      keywords: ["读书笔记", "ddia", "复制"],
      workspace_id: "wks_mock_notes",
      occurred_at: ago(4 * DAY),
      created_at: ago(4 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-10",
      kind: "fact",
      content:
        "Bot 的 USER.md 最多 2,000 个字符，由所有 Bot 共享；记忆条目同样在本设备的所有 Bot 之间共享。",
      keywords: ["user.md", "记忆", "共享"],
      importance: "normal",
      created_at: ago(30 * DAY),
      updated_at: ago(30 * DAY),
      last_read_at: ago(10 * DAY),
    }),
    memory({
      id: "mem_019a7c1e-11",
      kind: "observation",
      content:
        "最近三次 Codex 会话在长时间编译后出现输出延迟，可能和本机 CPU 温控降频有关，需要更多样本确认。",
      keywords: ["codex", "性能", "编译"],
      workspace_id: "wks_mock_pontia",
      organized: false,
      importance: "low",
      created_at: ago(2 * 60),
    }),
    memory({
      id: "mem_019a7c1e-12",
      kind: "decision",
      content: "Dashboard 设置页的“外观”和“语言”只保存在当前浏览器中。",
      keywords: ["settings", "外观", "语言"],
      workspace_id: "wks_mock_pontia",
      archived: true,
      created_at: ago(40 * DAY),
      updated_at: ago(7 * DAY),
      created_by: "bot",
      sources: "已合并到 mem_019a7c1e-01。",
    }),
  ];
  return {
    user: { text: userText, updated_at: ago(3 * DAY) },
    memories,
  };
}

export function createEmptyMemoryData(): MockMemoryData {
  return { user: { text: "", updated_at: null }, memories: [] };
}
