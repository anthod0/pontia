import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { SessionModel } from "../../src/api/types";
import {
  effortChipColumns,
  effortChoices,
  meterBars,
  pickerRows,
  sessionCreationOption,
} from "../../src/components/chat/modelEffort";
import type { SessionView } from "../../src/api/types";
import Harness from "./ModelEffortPickerHarness.svelte";

const effort = (id: string, description = "") => ({ id, name: id, description });
const models: SessionModel[] = [
  {
    id: "max",
    name: "Max",
    description: "",
    reasoning_efforts: [effort("low"), effort("medium", "Balanced"), effort("high")],
    default_reasoning_effort: "medium",
  },
  { id: "mini", name: "Mini", description: "", reasoning_efforts: [effort("low")] },
  { id: "plain", name: "Plain", description: "", reasoning_efforts: [] },
];

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models,
    version: null,
    fetched_at: Date.now(),
    default_model: "max",
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  document.body.style.pointerEvents = "";
});

test("lists the client default row only without a search and filters by name and ID", () => {
  const rows = (search: string, allowDefault = true) =>
    pickerRows(models, models[0], allowDefault, search).map((row) => row.key);
  expect(rows("")).toEqual(["", "max", "mini", "plain"]);
  expect(rows("", false)).toEqual(["max", "mini", "plain"]);
  expect(rows(" PLA ")).toEqual(["plain"]);
});

test("matches a session to the creation option that owns its model catalog", () => {
  const active = {
    client_type: "acp",
    metadata: { acp_agent_id: "grok" },
  } as SessionView;
  const options = [
    {
      id: "kiro",
      name: "Kiro",
      client_type: "acp",
      model_catalog_key: "kiro",
      metadata: { acp_agent_id: "kiro" },
      version: null,
    },
    {
      id: "grok",
      name: "Grok",
      client_type: "acp",
      model_catalog_key: "grok",
      metadata: { acp_agent_id: "grok" },
      version: null,
    },
  ];

  expect(sessionCreationOption(active, options)?.model_catalog_key).toBe("grok");
});

test("offers the default effort before model options and nothing for effortless models", () => {
  expect(effortChoices(models[0])).toEqual(["", "low", "medium", "high"]);
  expect(effortChoices(models[2])).toEqual([]);
  expect(effortChoices(undefined)).toEqual([]);
});

test("maps effort order onto four meter bars", () => {
  const lit = (index: number, count: number) => meterBars(index, count).filter(Boolean).length;
  expect([0, 1, 2].map((index) => lit(index, 3))).toEqual([1, 3, 4]);
  expect(lit(0, 1)).toBe(1);
  expect(meterBars(4, 5)).toHaveLength(4);
});

test("wraps mobile effort chips into three columns above five choices", () => {
  expect(effortChipColumns(5)).toBe("auto repeat(4, minmax(0, 1fr))");
  expect(effortChipColumns(6)).toBe("repeat(3, minmax(0, 1fr))");
});

test("keeps the panel open on a model with efforts and confirms on an effort", async () => {
  const onchange = vi.fn();
  render(Harness, { onchange });
  const trigger = document.getElementById("picker")!;
  await waitFor(() => expect(trigger).toHaveTextContent("Max"));
  expect(trigger).toHaveTextContent("Default · medium");
  await fireEvent.click(trigger);
  await fireEvent.click(await screen.findByLabelText("Mini"));
  const efforts = screen.getByLabelText("Reasoning effort");
  expect(screen.getByLabelText("Available models")).toBeInTheDocument();
  expect(within(efforts).getByLabelText("Default")).toHaveTextContent("Client default");
  await fireEvent.click(within(efforts).getByLabelText("low"));
  await waitFor(() => expect(screen.queryByLabelText("Available models")).toBeNull());
  expect(onchange).toHaveBeenLastCalledWith("mini", "low");
  expect(trigger).toHaveTextContent("Mini");
  expect(trigger).toHaveTextContent("low");
});

test("follows the hovered model and closes directly on a model without efforts", async () => {
  const onchange = vi.fn();
  render(Harness, { model: "max", effort: "high", onchange });
  const trigger = document.getElementById("picker")!;
  await waitFor(() => expect(trigger).toHaveTextContent("high"));
  await fireEvent.click(trigger);
  const efforts = await screen.findByLabelText("Reasoning effort");
  expect(within(efforts).getByLabelText("high")).toHaveAttribute("aria-selected", "true");
  expect(within(efforts).getByLabelText("medium")).toHaveTextContent("Model default");
  expect(within(efforts).getByLabelText("medium")).toHaveTextContent("Balanced");
  await fireEvent.pointerMove(screen.getByLabelText("Plain"));
  expect(screen.queryByLabelText("Reasoning effort")).toBeNull();
  expect(screen.getByText("No reasoning effort")).toBeInTheDocument();
  await fireEvent.click(screen.getByLabelText("Plain"));
  await waitFor(() => expect(screen.queryByLabelText("Available models")).toBeNull());
  expect(onchange).toHaveBeenLastCalledWith("plain", "");
  expect(trigger).toHaveTextContent("No reasoning effort");
});

test("navigates models and efforts from the keyboard", async () => {
  const onchange = vi.fn();
  render(Harness, { allowDefault: true, onchange });
  const trigger = document.getElementById("picker")!;
  await waitFor(() => expect(trigger).toHaveTextContent("Max · Default"));
  await fireEvent.click(trigger);
  const search = await screen.findByLabelText("Search models");
  const active = () => document.getElementById(search.getAttribute("aria-activedescendant")!);
  expect(active()).toHaveAccessibleName("Client default model");
  await fireEvent.keyDown(search, { key: "ArrowDown" });
  await fireEvent.keyDown(search, { key: "ArrowDown" });
  expect(active()).toHaveAttribute("aria-label", "Mini");
  await fireEvent.keyDown(search, { key: "ArrowUp" });
  await fireEvent.keyDown(search, { key: "ArrowRight" });
  expect(active()).toHaveAttribute("aria-label", "Default");
  await fireEvent.keyDown(search, { key: "ArrowDown" });
  await fireEvent.keyDown(search, { key: "ArrowDown" });
  expect(active()).toHaveAttribute("aria-label", "medium");
  await fireEvent.keyDown(search, { key: "ArrowLeft" });
  expect(active()).toHaveAttribute("aria-label", "Max");
  await fireEvent.keyDown(search, { key: "ArrowRight" });
  await fireEvent.keyDown(search, { key: "ArrowUp" });
  await fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.queryByLabelText("Search models")).toBeNull());
  expect(onchange).toHaveBeenLastCalledWith("max", "high");
  expect(trigger).toHaveTextContent("Max · high");
});

test("a cancelled sheet drag keeps the picker open and a downward release dismisses it", async () => {
  const matchMedia = window.matchMedia;
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    ...matchMedia(query),
    matches: query.includes("max-width: 767px"),
  }));
  render(Harness, { onchange: vi.fn() });
  await fireEvent.click(document.getElementById("picker")!);
  const sheet = await screen.findByRole("dialog");
  const handle = within(sheet).getByRole("presentation");
  Object.defineProperty(handle, "setPointerCapture", { value: vi.fn() });
  const pointer = { pointerId: 1, isPrimary: true, pointerType: "touch" };
  await fireEvent.pointerDown(handle, { ...pointer, clientY: 100 });
  await fireEvent.pointerMove(handle, { ...pointer, clientY: 210 });
  await fireEvent.pointerCancel(handle, { ...pointer, clientY: 210 });
  expect(sheet).toBeVisible();
  await fireEvent.pointerDown(handle, { ...pointer, clientY: 100 });
  await fireEvent.pointerUp(handle, { ...pointer, clientY: 210 });
  await waitFor(() => expect(sheet).not.toBeInTheDocument());
});

test("on mobile switches models in place and commits efforts until done", async () => {
  const matchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({
    ...matchMedia(query),
    matches: true,
  })) as typeof matchMedia;
  try {
    const onchange = vi.fn();
    render(Harness, { model: "max", effort: "high", onchange });
    const trigger = document.getElementById("picker")!;
    await waitFor(() => expect(trigger).toHaveTextContent("high"));
    await fireEvent.click(trigger);
    const sheet = await screen.findByRole("dialog", { name: "Model and reasoning effort" });
    const chips = within(sheet).getByRole("radiogroup", { name: "Reasoning effort" });
    expect(within(chips).getByRole("radio", { name: "high" })).toBeChecked();
    await fireEvent.click(within(sheet).getByRole("option", { name: "Mini" }));
    expect(within(chips).getByRole("radio", { name: "Default" })).toBeChecked();
    expect(
      within(chips)
        .getAllByRole("radio")
        .map((chip) => chip.textContent),
    ).toEqual(["Default", "low"]);
    await fireEvent.click(within(sheet).getByRole("option", { name: "Plain" }));
    expect(within(sheet).queryByRole("radiogroup")).toBeNull();
    expect(within(sheet).getByText("Plain has no reasoning effort")).toBeInTheDocument();
    expect(within(sheet).getByRole("option", { name: "Plain" })).toHaveTextContent("No effort");
    await fireEvent.click(within(sheet).getByRole("option", { name: "Max" }));
    await fireEvent.click(within(sheet).getByRole("radio", { name: "low" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await fireEvent.click(within(sheet).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(onchange).toHaveBeenLastCalledWith("max", "low");
  } finally {
    window.matchMedia = matchMedia;
  }
});
