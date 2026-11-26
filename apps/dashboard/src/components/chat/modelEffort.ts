import type { SessionCreationOption, SessionModel, SessionView } from "../../api/types";

export function sessionCreationOption(
  session: SessionView,
  options: SessionCreationOption[],
): SessionCreationOption | undefined {
  return options.find(
    (option) =>
      option.client_type === session.client_type &&
      Object.entries(option.metadata).every(([key, value]) => session.metadata[key] === value),
  );
}

/**
 * A model row in the picker. `key` is the bound model value; the client default row uses `''`
 * and offers the efforts of the catalog default model when it is known.
 */
export type PickerRow =
  | { key: ""; clientDefault: true; model: SessionModel | undefined }
  | { key: string; clientDefault: false; model: SessionModel };

export function pickerRows(
  models: SessionModel[],
  defaultModel: SessionModel | undefined,
  allowDefault: boolean,
  search: string,
): PickerRow[] {
  const query = search.trim().toLowerCase();
  const matches = models
    .filter((model) => `${model.name} ${model.id}`.toLowerCase().includes(query))
    .map((model): PickerRow => ({ key: model.id, clientDefault: false, model }));
  return allowDefault && !query
    ? [{ key: "", clientDefault: true, model: defaultModel }, ...matches]
    : matches;
}

/** Effort values offered for a model: `''` (default) followed by its own options. */
export function effortChoices(model: SessionModel | undefined): string[] {
  const options = model?.reasoning_efforts ?? [];
  return options.length ? ["", ...options.map((option) => option.id)] : [];
}

export const meterBarCount = 4;

/** Maps an option's position to lit bars; the scale only reflects option order. */
export function meterBars(index: number, count: number): boolean[] {
  const lit = count <= 1 ? 0 : Math.round((index / (count - 1)) * (meterBarCount - 1));
  return Array.from({ length: meterBarCount }, (_, bar) => bar <= lit);
}

/** Mobile effort chips fit on one row up to five choices, otherwise wrap into three columns. */
export function effortChipColumns(choices: number): string {
  return choices > 5 ? "repeat(3, minmax(0, 1fr))" : `auto repeat(${choices - 1}, minmax(0, 1fr))`;
}
