import { dashboardClient } from "../../clients";
import { readable } from "svelte/store";
import { readStoredWith, writeStored } from "../../lib/storage";
import { profileIdPattern } from "./form";
import {
  builtInProfileTemplates,
  type ProfileTemplate,
  type ProfileTemplateText,
} from "./templates";

/** Pontia Cloud publishes the shared catalog from `assets/agent-profile-templates.json`. */
export const publishedTemplatesUrl = "https://pontia.dev/agent-profile-templates.json";
export const templatesSchemaVersion = 1;

const storageKey = "pontia.profileTemplates";
const refreshInterval = 24 * 60 * 60 * 1000;
const requestTimeout = 5_000;

type SavedTemplates = { fetched_at: number; document: unknown };

function isText(value: unknown): value is ProfileTemplateText {
  if (typeof value !== "object" || value === null) return false;
  const { name, description, system_prompt } = value as Record<string, unknown>;
  return [name, description, system_prompt].every(
    (field) => typeof field === "string" && field.trim() !== "",
  );
}

function parseTemplate(value: unknown): ProfileTemplate | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const { id, icon, client_type, client_key, model, reasoning_effort, use_yolo, text } =
    value as Record<string, unknown>;
  const texts = text as Partial<Record<string, unknown>> | null | undefined;
  if (typeof texts !== "object" || texts === null) return undefined;
  const { en } = texts;
  const valid =
    typeof id === "string" &&
    profileIdPattern.test(id) &&
    typeof icon === "string" &&
    typeof client_type === "string" &&
    (client_key === undefined
      ? !dashboardClient(client_type)?.requiresClientKey
      : typeof client_key === "string" && client_key.trim() !== "") &&
    typeof model === "string" &&
    model !== "" &&
    (reasoning_effort === undefined || typeof reasoning_effort === "string") &&
    typeof use_yolo === "boolean" &&
    isText(en);
  if (!valid) return undefined;
  return {
    id,
    icon,
    client_type,
    ...(typeof client_key === "string" ? { client_key } : {}),
    model,
    ...(reasoning_effort === undefined ? {} : { reasoning_effort }),
    use_yolo,
    text: { en },
  };
}

/**
 * The templates in a published document, or undefined when the document does not match the
 * schema. Templates for clients this Dashboard does not know are left out.
 */
export function parseTemplatesDocument(document: unknown): ProfileTemplate[] | undefined {
  if (typeof document !== "object" || document === null) return undefined;
  const { schema_version, templates } = document as Record<string, unknown>;
  if (schema_version !== templatesSchemaVersion || !Array.isArray(templates)) return undefined;
  const parsed = templates.map(parseTemplate);
  if (!parsed.every((template) => template !== undefined)) return undefined;
  if (new Set(parsed.map((template) => template.id)).size !== parsed.length) return undefined;
  const known = parsed.filter((template) => dashboardClient(template.client_type));
  return known.length > 0 ? known : undefined;
}

function readSaved(): { fetched_at: number; templates: ProfileTemplate[] } | null {
  return readStoredWith(storageKey, (saved) => {
    const entry = JSON.parse(saved) as Partial<SavedTemplates> | null;
    if (typeof entry?.fetched_at !== "number") return undefined;
    const templates = parseTemplatesDocument(entry.document);
    return templates && { fetched_at: entry.fetched_at, templates };
  });
}

async function fetchPublished(): Promise<ProfileTemplate[] | undefined> {
  try {
    const response = await fetch(publishedTemplatesUrl, {
      credentials: "omit",
      signal: AbortSignal.timeout(requestTimeout),
    });
    if (!response.ok) return undefined;
    const document: unknown = await response.json();
    const templates = parseTemplatesDocument(document);
    if (templates) {
      const saved: SavedTemplates = { fetched_at: Date.now(), document };
      writeStored(storageKey, JSON.stringify(saved));
    }
    return templates;
  } catch {
    // Offline, timed out or blocked: the templates already shown stay.
    return undefined;
  }
}

/**
 * The profile templates to offer: the published ones saved in the last day, refreshed in the
 * background once a day, and the built-in ones until published templates have been loaded.
 */
export const profileTemplates = readable<ProfileTemplate[]>(builtInProfileTemplates, (set) => {
  const saved = readSaved();
  if (saved) set(saved.templates);
  if (saved && Date.now() - saved.fetched_at < refreshInterval) return;
  let active = true;
  void fetchPublished().then((templates) => {
    if (active && templates) set(templates);
  });
  return () => {
    active = false;
  };
});
