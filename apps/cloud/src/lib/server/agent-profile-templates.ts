import templatesDocument from "$assets/agent-profile-templates.json";

type Template = Omit<(typeof templatesDocument.templates)[number], "reasoning_effort"> & {
  reasoning_effort?: string;
};

export const AGENT_PROFILE_TEMPLATES_CACHE_CONTROL = "public, max-age=300";

export function agentProfileTemplatesHeaders(contentType: string): Record<string, string> {
  return {
    "content-type": contentType,
    "cache-control": AGENT_PROFILE_TEMPLATES_CACHE_CONTROL,
    "access-control-allow-origin": "*",
  };
}

export function renderAgentProfileTemplatesJson(document = templatesDocument): string {
  return JSON.stringify(document, null, 2) + "\n";
}

export function renderAgentProfileTemplatesMarkdown(
  document: { templates: Template[] } = templatesDocument,
): string {
  const lines = [
    "# Agent Profile Templates",
    "",
    "Use these templates as starting points when creating shared Agent Profiles with `pontia bot profile create`.",
    "Check available clients and models with `pontia bot profile clients` and `pontia bot profile models --client TYPE/KEY`, then customize fields for the user's requirements or keep template defaults when unspecified.",
  ];
  for (const template of document.templates) {
    const { name, description, system_prompt } = template.text.en;
    const ticks = system_prompt.match(/`+/g) ?? [];
    const fence = "`".repeat(ticks.reduce((size, run) => Math.max(size, run.length + 1), 3));
    lines.push(
      "",
      `## ${name} (\`${template.id}\`)`,
      "",
      `- Profile ID: \`${template.id}\``,
      `- Client: \`${template.client_type}/${template.client_key ?? "default"}\``,
      `- Model: \`${template.model}\``,
    );
    if (template.reasoning_effort)
      lines.push(`- Reasoning effort: \`${template.reasoning_effort}\``);
    lines.push(
      `- YOLO mode: \`${template.use_yolo}\``,
      "",
      "### Description",
      "",
      description,
      "",
      "### System prompt",
      "",
      fence + "text",
      system_prompt,
      fence,
    );
  }
  return lines.join("\n") + "\n";
}
