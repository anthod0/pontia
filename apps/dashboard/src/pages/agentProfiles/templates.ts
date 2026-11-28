import { dashboardClient } from "../../clients";
import type { Component } from "svelte";
import BugIcon from "phosphor-svelte/lib/BugIcon";
import CodeIcon from "phosphor-svelte/lib/CodeIcon";
import GitDiffIcon from "phosphor-svelte/lib/GitDiffIcon";
import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
import PencilSimpleLineIcon from "phosphor-svelte/lib/PencilSimpleLineIcon";
import GlobeHemisphereWestIcon from "phosphor-svelte/lib/GlobeHemisphereWestIcon";
import PenNibIcon from "phosphor-svelte/lib/PenNibIcon";
import TreeStructureIcon from "phosphor-svelte/lib/TreeStructureIcon";
import PaletteIcon from "phosphor-svelte/lib/PaletteIcon";
import FilmSlateIcon from "phosphor-svelte/lib/FilmSlateIcon";
import RobotIcon from "phosphor-svelte/lib/RobotIcon";
import TestTubeIcon from "phosphor-svelte/lib/TestTubeIcon";
import builtInDocument from "$assets/agent-profile-templates.json";
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
  client_key?: string;
  model: string;
  reasoning_effort?: string;
  use_yolo: boolean;
  text: { en: ProfileTemplateText };
}

export function templateClientName(template: ProfileTemplate, clients: AgentClientView[]) {
  return (
    dashboardClient(template.client_type)?.displayName(
      template.client_key,
      matchingClient(template, clients)?.name,
    ) ?? template.client_type
  );
}

const templateIcons: Record<string, Component> = {
  "globe-hemisphere-west": GlobeHemisphereWestIcon,
  "pen-nib": PenNibIcon,
  "tree-structure": TreeStructureIcon,
  palette: PaletteIcon,
  "film-slate": FilmSlateIcon,
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
export const builtInProfileTemplates: ProfileTemplate[] = builtInDocument.templates;

export type TemplateClientStatus = "available" | "disabled" | "missing";

function matchingClient(template: ProfileTemplate, clients: AgentClientView[]) {
  return clients.find(
    (client) =>
      client.client_type === template.client_type &&
      client.client_key === (template.client_key ?? "default"),
  );
}

/** The enabled client a template launches with, if this device offers one. */
export function templateClient(template: ProfileTemplate, clients: AgentClientView[]) {
  const client = matchingClient(template, clients);
  return client?.enabled ? client : undefined;
}

/** Whether a template's client is enabled, detected but disabled, or not installed. */
export function templateClientStatus(
  template: ProfileTemplate,
  clients: AgentClientView[],
): TemplateClientStatus {
  const client = matchingClient(template, clients);
  if (client?.enabled) return "available";
  return client?.detection.status === "detected" ? "disabled" : "missing";
}

/**
 * Fills a draft from a template. The client and model are kept when the template's client is not
 * enabled; otherwise the template's model is used until its catalog says otherwise.
 */
export function applyProfileTemplate(
  draft: AgentProfileDraft,
  template: ProfileTemplate,
  clients: AgentClientView[],
): AgentProfileDraft {
  const text = template.text.en;
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
