import { expect, test } from "bun:test";
import { renderSitemap } from "../src/lib/server/sitemap";
import templates from "$assets/agent-profile-templates.json";
import fixture from "../../../crates/pontia/tests/fixtures/profile-templates/fenced.json";
import { renderAgentProfileTemplatesMarkdown } from "../src/lib/server/agent-profile-templates";
import { GET as json } from "../src/routes/agent-profile-templates.json/+server";
import { GET as markdown } from "../src/routes/agent-profile-templates.md/+server";

test("publishes the shared catalog as cacheable cross-origin JSON and Markdown", async () => {
  for (const [response, type] of [
    [json(), "application/json"],
    [markdown(), "text/markdown"],
  ] as const) {
    expect(response.headers.get("content-type")).toBe(type + "; charset=utf-8");
    expect(response.headers.get("cache-control")).toBe("public, max-age=300");
    expect(response.headers.get("access-control-allow-origin")).toBe("*");
  }
  expect(await json().text()).toBe(JSON.stringify(templates, null, 2) + "\n");
  const body = await markdown().text();
  for (const template of templates.templates) {
    expect(body).toContain(`## ${template.text.en.name} (\`${template.id}\`)`);
    if (template.reasoning_effort)
      expect(body).toContain(`- Reasoning effort: \`${template.reasoning_effort}\``);
  }
});

test("renders prompts with nested fences and omits unspecified reasoning effort", async () => {
  const expected = Bun.file(
    new URL("../../../crates/pontia/tests/fixtures/profile-templates/fenced.md", import.meta.url),
  );
  expect(renderAgentProfileTemplatesMarkdown(fixture)).toBe(await expected.text());
});

test("keeps standalone template URLs outside the documentation indexes", async () => {
  const llms = await Bun.file(new URL("../static/llms.txt", import.meta.url)).text();
  for (const index of [llms, renderSitemap(["setup"], [])]) {
    expect(index).not.toContain("/agent-profile-templates.");
  }
});
