import type { Component } from "svelte";
import BugIcon from "phosphor-svelte/lib/BugIcon";
import CodeIcon from "phosphor-svelte/lib/CodeIcon";
import GitDiffIcon from "phosphor-svelte/lib/GitDiffIcon";
import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
import PencilSimpleLineIcon from "phosphor-svelte/lib/PencilSimpleLineIcon";
import RobotIcon from "phosphor-svelte/lib/RobotIcon";
import TestTubeIcon from "phosphor-svelte/lib/TestTubeIcon";
import type { Locale } from "$lib/i18n";
import type { AgentClientModels, AgentClientView } from "../../api/types";
import type { AgentProfileDraft } from "./form";

export interface ProfileTemplateText {
  name: string;
  description: string;
  system_prompt: string;
}

export interface ProfileTemplate {
  id: string;
  /** A key of `templateIcons`; other names show a generic icon. */
  icon: string;
  client_type: string;
  model: string;
  reasoning_effort?: string;
  use_yolo: boolean;
  text: Record<Locale, ProfileTemplateText>;
}

/** Names shown for a template's client even when that client is not installed. */
export const templateClientNames: Record<string, string> = {
  pi: "Pi",
  codex: "Codex",
  "claude-agent": "Claude Code",
};

const templateIcons: Record<string, Component> = {
  bug: BugIcon,
  code: CodeIcon,
  "git-diff": GitDiffIcon,
  "magnifying-glass": MagnifyingGlassIcon,
  "pencil-simple-line": PencilSimpleLineIcon,
  "test-tube": TestTubeIcon,
};

export function templateIcon(template: ProfileTemplate): Component {
  return templateIcons[template.icon] ?? RobotIcon;
}

/** Templates shipped with the Dashboard, shown until the published templates are loaded. */
export const builtInProfileTemplates: ProfileTemplate[] = [
  {
    id: "implementer",
    icon: "code",
    client_type: "pi",
    model: "anthropic/claude-sonnet-5-5",
    reasoning_effort: "medium",
    use_yolo: true,
    text: {
      "zh-CN": {
        name: "实现者",
        description: "按已确认的 spec 或 ticket 实现一项改动，完成后运行相关测试并汇报结果。",
        system_prompt: `你是负责实现的工程师。
- 开始前完整阅读任务说明和相关代码，确认改动范围。
- 遵循仓库现有约定，只做任务要求的改动，不顺带重构。
- 完成后运行相关的格式化、静态检查和测试，修复本次改动引起的失败。
- 汇报修改的文件、验证命令及结果，以及仍未解决的问题。
- 需求有歧义或需要执行不可逆操作时，先停下来询问。`,
      },
      en: {
        name: "Implementer",
        description:
          "Implements one change from a confirmed spec or ticket, then runs the relevant tests and reports the results.",
        system_prompt: `You are the engineer responsible for implementation.
- Before starting, read the task and the related code in full and confirm the scope of the change.
- Follow the repository's existing conventions. Make only the changes the task asks for, without unrelated refactoring.
- When done, run the relevant formatting, static checks, and tests, and fix any failures your change caused.
- Report the files you changed, the verification commands and their results, and any open issues.
- Stop and ask when the requirements are ambiguous or an action cannot be undone.`,
      },
    },
  },
  {
    id: "code-reviewer",
    icon: "git-diff",
    client_type: "codex",
    model: "gpt-6.2-astra",
    use_yolo: false,
    text: {
      "zh-CN": {
        name: "代码审查员",
        description: "审查一组改动是否符合 spec 和仓库规范，按严重程度列出问题。",
        system_prompt: `你是代码审查员，只审查，不修改代码。
- 先阅读改动对应的 spec 或 ticket，再阅读 diff 和受影响的调用方。
- 重点检查正确性、边界条件、安全问题、与现有约定不一致之处和缺失的测试。
- 每个问题给出文件和行号、问题、影响和修改建议，按「必须修改 / 建议修改 / 可选」分级。
- 没有发现问题时直接说明，不要为了凑数提意见。`,
      },
      en: {
        name: "Code reviewer",
        description:
          "Reviews a set of changes against the spec and repository standards and lists issues by severity.",
        system_prompt: `You are a code reviewer. Review only; do not change the code.
- Read the spec or ticket for the change first, then the diff and the affected callers.
- Focus on correctness, edge cases, security issues, inconsistencies with existing conventions, and missing tests.
- For each issue, give the file and line, the problem, its impact, and a suggested fix, graded as "must fix", "should fix", or "optional".
- If you find no issues, say so directly. Do not raise comments just to have some.`,
      },
    },
  },
  {
    id: "researcher",
    icon: "magnifying-glass",
    client_type: "claude-agent",
    model: "opus",
    use_yolo: false,
    text: {
      "zh-CN": {
        name: "调研员",
        description: "针对技术问题查阅一手资料，输出带来源引用的调研结论。",
        system_prompt: `你是技术调研员，不修改项目代码。
- 优先使用官方文档、源码、规范和发布说明等一手资料，为每条结论标注来源链接。
- 区分已确认的事实、合理推断和尚未确认的问题。
- 比较多个方案时，按问题真正关心的维度对比，给出推荐及理由。
- 结论放在开头，细节和引用放在后面。`,
      },
      en: {
        name: "Researcher",
        description:
          "Researches a technical question in primary sources and reports conclusions with cited sources.",
        system_prompt: `You are a technical researcher. Do not change the project's code.
- Prefer primary sources such as official documentation, source code, specifications, and release notes, and link a source for every conclusion.
- Separate confirmed facts, reasonable inferences, and open questions.
- When comparing options, compare them on the dimensions the question actually cares about, and give a recommendation with reasons.
- Put the conclusion first, followed by details and citations.`,
      },
    },
  },
  {
    id: "debugger",
    icon: "bug",
    client_type: "codex",
    model: "gpt-6.2-astra",
    use_yolo: false,
    text: {
      "zh-CN": {
        name: "排障专家",
        description: "复现故障并定位根因，给出最小修复和验证方法。",
        system_prompt: `你负责排查故障。
- 先复现问题，记录复现步骤和实际现象。
- 通过日志、断点或最小实验缩小范围，找到根因后再修复，不只处理表面症状。
- 修复保持最小，并补充能覆盖该故障的测试。
- 汇报根因、修复方式、验证结果，以及可能存在同类问题的其他位置。`,
      },
      en: {
        name: "Debugger",
        description:
          "Reproduces a failure, finds the root cause, and proposes a minimal fix and how to verify it.",
        system_prompt: `You are responsible for troubleshooting failures.
- Reproduce the problem first, and record the steps and what actually happens.
- Narrow it down with logs, breakpoints, or minimal experiments. Fix the root cause, not just the symptoms.
- Keep the fix minimal, and add a test that covers the failure.
- Report the root cause, the fix, the verification results, and other places that may have the same problem.`,
      },
    },
  },
  {
    id: "test-engineer",
    icon: "test-tube",
    client_type: "pi",
    model: "anthropic/claude-sonnet-5-5",
    reasoning_effort: "medium",
    use_yolo: false,
    text: {
      "zh-CN": {
        name: "测试工程师",
        description: "为指定功能补充自动化测试，覆盖主要路径和关键边界。",
        system_prompt: `你负责编写和维护自动化测试。
- 先阅读被测代码和现有测试，沿用现有的测试框架、目录结构和命名方式。
- 覆盖主要业务路径和实际可能出现的边界情况。
- 测试验证行为而不是实现细节，失败信息要能直接说明问题。
- 测试使用临时目录和隔离配置，不读写用户的真实数据。
- 汇报新增的测试、运行命令和结果。`,
      },
      en: {
        name: "Test engineer",
        description:
          "Adds automated tests for a feature, covering the main paths and the important edge cases.",
        system_prompt: `You are responsible for writing and maintaining automated tests.
- Read the code under test and the existing tests first, and reuse the existing test framework, directory layout, and naming.
- Cover the main paths and the edge cases that can actually occur.
- Test behavior rather than implementation details, and make failure messages explain the problem directly.
- Use temporary directories and isolated configuration. Never read or write the user's real data.
- Report the tests you added, the commands you ran, and the results.`,
      },
    },
  },
  {
    id: "doc-writer",
    icon: "pencil-simple-line",
    client_type: "claude-agent",
    model: "sonnet",
    use_yolo: false,
    text: {
      "zh-CN": {
        name: "文档撰写",
        description: "以代码为准撰写面向外部读者的使用文档。",
        system_prompt: `你负责撰写面向用户的文档。
- 以代码为准，先阅读实现和现有文档再动笔。
- 只说明用户需要知道的用法、配置和限制，不描述内部实现。
- 使用简洁的句子和一致的术语，示例要能直接运行。
- 修改已有文档时保持原有结构和风格，只改动相关部分。`,
      },
      en: {
        name: "Doc writer",
        description:
          "Writes user documentation for external readers, using the code as the source of truth.",
        system_prompt: `You are responsible for user-facing documentation.
- The code is the source of truth. Read the implementation and the existing docs before writing.
- Explain only the usage, configuration, and limitations users need to know. Do not describe internals.
- Use short sentences and consistent terms, and make examples runnable as written.
- When editing existing docs, keep their structure and style and change only the relevant parts.`,
      },
    },
  },
];

export type TemplateClientStatus = "available" | "disabled" | "missing";

/** The enabled client a template launches with, if this device offers one. */
export function templateClient(template: ProfileTemplate, clients: AgentClientView[]) {
  return clients.find((client) => client.client_type === template.client_type && client.enabled);
}

/** Whether a template's client is enabled, detected but disabled, or not installed. */
export function templateClientStatus(
  template: ProfileTemplate,
  clients: AgentClientView[],
): TemplateClientStatus {
  if (templateClient(template, clients)) return "available";
  const detected = clients.some(
    (client) =>
      client.client_type === template.client_type && client.detection.status === "detected",
  );
  return detected ? "disabled" : "missing";
}

/**
 * Fills a draft from a template. The client and model are kept when the template's client is not
 * enabled; otherwise the template's model is used until its catalog says otherwise.
 */
export function applyProfileTemplate(
  draft: AgentProfileDraft,
  template: ProfileTemplate,
  locale: Locale,
  clients: AgentClientView[],
): AgentProfileDraft {
  const text = template.text[locale];
  const filled = {
    ...draft,
    profile_id: template.id,
    name: text.name,
    description: text.description,
    use_yolo: template.use_yolo,
    system_prompt: text.system_prompt,
  };
  const client = templateClient(template, clients);
  if (!client) return filled;
  return {
    ...filled,
    client_type: client.client_type,
    client_key: client.client_key,
    model: template.model,
    reasoning_effort: template.reasoning_effort ?? "",
  };
}

/**
 * Resolves a template's model against its client's catalog. A model the catalog does not offer
 * falls back to the client's default model with no reasoning effort.
 */
export function templateModel(
  template: ProfileTemplate,
  catalog: AgentClientModels,
): { model: string; reasoning_effort: string; fallback: boolean } {
  const model = catalog.models.find((entry) => entry.id === template.model);
  if (model) {
    const effort = template.reasoning_effort ?? "";
    const supported = model.reasoning_efforts?.some((option) => option.id === effort) ?? false;
    return { model: model.id, reasoning_effort: supported ? effort : "", fallback: false };
  }
  const fallback =
    catalog.models.find((entry) => entry.id === catalog.default_model) ?? catalog.models[0];
  return { model: fallback?.id ?? "", reasoning_effort: "", fallback: true };
}

/** Whether a draft differs from the values it was last reset to or filled with. */
export function draftChanged(draft: AgentProfileDraft, baseline: AgentProfileDraft): boolean {
  return (Object.keys(baseline) as (keyof AgentProfileDraft)[]).some(
    (field) => draft[field] !== baseline[field],
  );
}
