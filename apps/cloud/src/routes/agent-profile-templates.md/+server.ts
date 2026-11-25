import {
  agentProfileTemplatesHeaders,
  renderAgentProfileTemplatesMarkdown,
} from "$lib/server/agent-profile-templates";

export const prerender = true;

export function GET(): Response {
  return new Response(renderAgentProfileTemplatesMarkdown(), {
    headers: agentProfileTemplatesHeaders("text/markdown; charset=utf-8"),
  });
}
