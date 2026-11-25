import { expect, test } from "vitest";
import type { AgentClientModels, AgentClientView } from "../src/api/types";
import { createAgentProfileDraft } from "../src/pages/agentProfiles/form";
import {
  applyProfileTemplate,
  draftChanged,
  type ProfileTemplate,
  templateClientStatus,
  templateModel,
} from "../src/pages/agentProfiles/templates";

const template: ProfileTemplate = {
  id: "sample",
  icon: "code",
  client_type: "codex",
  model: "sample-model",
  reasoning_effort: "medium",
  use_yolo: true,
  text: {
    en: { name: "Sample", description: "Sample description", system_prompt: "Sample prompt" },
  },
};

function client(
  client_type: string,
  {
    enabled = true,
    detected = true,
    key = "default",
  }: { enabled?: boolean; detected?: boolean; key?: string } = {},
): AgentClientView {
  return {
    id: client_type,
    client_type,
    client_key: key,
    name: client_type,
    short_id: client_type,
    enabled,
    detection: { status: detected ? "detected" : "not_detected", detail: null },
    setup: { status: "not_required" },
    setup_command: null,
    version: null,
    version_checked_at: null,
    has_record: false,
    created_at: null,
    updated_at: null,
  };
}

function catalog(models: { id: string; efforts?: string[] }[], default_model: string | null) {
  return {
    version: null,
    fetched_at: 0,
    default_model,
    models: models.map(({ id, efforts = [] }) => ({
      id,
      name: id,
      description: "",
      reasoning_efforts: efforts.map((effort) => ({ id: effort, name: effort, description: "" })),
    })),
  } satisfies AgentClientModels;
}

test("fills a draft with the model and client key of the selected ACP agent", () => {
  const clients = [client("acp", { key: "grok" }), client("acp", { key: "antigravity" })];
  const acp = { ...template, client_type: "acp", client_key: "antigravity" };
  expect(applyProfileTemplate(createAgentProfileDraft(), acp, clients)).toEqual({
    profile_id: "sample",
    name: "Sample",
    description: "Sample description",
    client_type: "acp",
    client_key: "antigravity",
    model: "sample-model",
    reasoning_effort: "medium",
    use_yolo: true,
    system_prompt: "Sample prompt",
  });
});

test("reports availability for the specific ACP agent", () => {
  const acp = { ...template, client_type: "acp", client_key: "grok" };
  const other = client("acp", { key: "antigravity" });
  expect(templateClientStatus(acp, [other])).toBe("missing");
  expect(templateClientStatus(acp, [other, client("acp", { key: "grok", enabled: false })])).toBe(
    "disabled",
  );
});

test("keeps the client and model when the template's client is not enabled", () => {
  const draft = {
    ...createAgentProfileDraft(),
    client_type: "pi",
    client_key: "default",
    model: "provider/model",
    reasoning_effort: "high",
  };
  const clients = [client("pi"), client("codex", { enabled: false })];
  expect(applyProfileTemplate(draft, template, clients)).toMatchObject({
    profile_id: "sample",
    name: "Sample",
    client_type: "pi",
    client_key: "default",
    model: "provider/model",
    reasoning_effort: "high",
  });
});

test("tells an enabled client from a disabled or missing one", () => {
  expect(templateClientStatus(template, [client("codex")])).toBe("available");
  expect(templateClientStatus(template, [client("codex", { enabled: false })])).toBe("disabled");
  expect(
    templateClientStatus(template, [client("codex", { enabled: false, detected: false })]),
  ).toBe("missing");
  expect(templateClientStatus(template, [client("pi")])).toBe("missing");
});

test("uses the template's model and effort only when the catalog offers them", () => {
  expect(
    templateModel(
      template,
      catalog([{ id: "other" }, { id: template.model, efforts: ["low", "medium"] }], "other"),
    ),
  ).toEqual({ model: template.model, reasoning_effort: "medium", fallback: false });
  expect(templateModel(template, catalog([{ id: template.model }], null))).toEqual({
    model: template.model,
    reasoning_effort: "",
    fallback: false,
  });
  expect(templateModel(template, catalog([{ id: "first" }, { id: "default" }], "default"))).toEqual(
    { model: "default", reasoning_effort: "", fallback: true },
  );
  expect(templateModel(template, catalog([{ id: "first" }], null))).toEqual({
    model: "first",
    reasoning_effort: "",
    fallback: true,
  });
});

test("reports a changed draft against its baseline", () => {
  const baseline = createAgentProfileDraft();
  expect(draftChanged({ ...baseline }, baseline)).toBe(false);
  expect(draftChanged({ ...baseline, system_prompt: "x" }, baseline)).toBe(true);
  expect(draftChanged({ ...baseline, use_yolo: true }, baseline)).toBe(true);
});
