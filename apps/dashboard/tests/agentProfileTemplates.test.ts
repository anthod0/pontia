import { expect, test } from "vitest";
import type { AgentClientModels, AgentClientView } from "../src/api/types";
import { createAgentProfileDraft } from "../src/pages/agentProfiles/form";
import {
  applyProfileTemplate,
  draftChanged,
  builtInProfileTemplates,
  templateClientNames,
  templateClientStatus,
  templateModel,
} from "../src/pages/agentProfiles/templates";

const template = (id: string) => builtInProfileTemplates.find((entry) => entry.id === id)!;

function client(
  client_type: string,
  { enabled = true, detected = true }: { enabled?: boolean; detected?: boolean } = {},
): AgentClientView {
  return {
    id: client_type,
    client_type,
    client_key: client_type,
    name: client_type,
    short_id: client_type,
    enabled,
    detection: { status: detected ? "detected" : "not_detected", detail: null },
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

test("every template has a valid profile ID, a known client, and text in both languages", () => {
  for (const entry of builtInProfileTemplates) {
    expect(entry.id).toMatch(/^[a-z0-9][a-z0-9._-]*$/);
    expect(templateClientNames[entry.client_type]).toBeTruthy();
    for (const text of Object.values(entry.text)) {
      expect(text.name && text.description && text.system_prompt).toBeTruthy();
      expect(text.system_prompt).not.toMatch(/\{\{|\}\}/);
    }
    expect(entry.text["zh-CN"].name).not.toBe(entry.text.en.name);
  }
  expect(new Set(builtInProfileTemplates.map((entry) => entry.id)).size).toBe(
    builtInProfileTemplates.length,
  );
  expect(
    builtInProfileTemplates.filter((entry) => entry.use_yolo).map((entry) => entry.id),
  ).toEqual(["implementer"]);
});

test("fills every field in the interface language when the template's client is enabled", () => {
  const draft = { ...createAgentProfileDraft(), client_type: "codex", client_key: "codex" };
  expect(applyProfileTemplate(draft, template("implementer"), "zh-CN", [client("pi")])).toEqual({
    profile_id: "implementer",
    name: "实现者",
    description: template("implementer").text["zh-CN"].description,
    client_type: "pi",
    client_key: "pi",
    model: "anthropic/claude-sonnet-5-5",
    reasoning_effort: "medium",
    use_yolo: true,
    system_prompt: template("implementer").text["zh-CN"].system_prompt,
  });
  expect(applyProfileTemplate(draft, template("implementer"), "en", [client("pi")]).name).toBe(
    "Implementer",
  );
});

test("keeps the client and model when the template's client is not enabled", () => {
  const draft = {
    ...createAgentProfileDraft(),
    client_type: "pi",
    client_key: "pi",
    model: "provider/model",
    reasoning_effort: "high",
  };
  const clients = [client("pi"), client("claude-agent", { enabled: false })];
  expect(applyProfileTemplate(draft, template("researcher"), "en", clients)).toMatchObject({
    profile_id: "researcher",
    name: "Researcher",
    client_type: "pi",
    client_key: "pi",
    model: "provider/model",
    reasoning_effort: "high",
  });
});

test("tells an enabled client from a disabled or missing one", () => {
  const researcher = template("researcher");
  expect(templateClientStatus(researcher, [client("claude-agent")])).toBe("available");
  expect(templateClientStatus(researcher, [client("claude-agent", { enabled: false })])).toBe(
    "disabled",
  );
  expect(
    templateClientStatus(researcher, [client("claude-agent", { enabled: false, detected: false })]),
  ).toBe("missing");
  expect(templateClientStatus(researcher, [client("pi")])).toBe("missing");
});

test("uses the template's model and effort only when the catalog offers them", () => {
  const implementer = template("implementer");
  expect(
    templateModel(
      implementer,
      catalog([{ id: "other" }, { id: implementer.model, efforts: ["low", "medium"] }], "other"),
    ),
  ).toEqual({ model: implementer.model, reasoning_effort: "medium", fallback: false });
  expect(templateModel(implementer, catalog([{ id: implementer.model }], null))).toEqual({
    model: implementer.model,
    reasoning_effort: "",
    fallback: false,
  });
  expect(
    templateModel(implementer, catalog([{ id: "first" }, { id: "default" }], "default")),
  ).toEqual({ model: "default", reasoning_effort: "", fallback: true });
  expect(templateModel(implementer, catalog([{ id: "first" }], null))).toEqual({
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
