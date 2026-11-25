import {
  agentProfileTemplatesHeaders,
  renderAgentProfileTemplatesJson,
} from "$lib/server/agent-profile-templates";

export const prerender = true;

export function GET(): Response {
  return new Response(renderAgentProfileTemplatesJson(), {
    headers: agentProfileTemplatesHeaders("application/json; charset=utf-8"),
  });
}
