import { mocks, session, turn, workspace } from "./fixtures";
import { testDevice } from "../../deviceFixture";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import type { CreateSessionResult } from "../../../src/api/types";

const NewChatPage = (await import("../../../src/pages/NewChatPage.svelte")).default;

test.each([undefined, "/office-mac"])(
  "opens Bot setup from workspace onboarding in the same device scope: %s",
  async (routeScope) => {
    mocks.workspaces.set([]);
    render(NewChatPage, { routeScope });
    const link = await screen.findByRole("link", { name: "Get started with a Bot" });
    expect(link).toHaveAttribute("href", `${routeScope ?? "/dashboard"}/bot`);
    await fireEvent.click(link);
    await waitFor(() =>
      expect(mocks.navigate).toHaveBeenCalledWith("/bot", undefined, { scope: routeScope }),
    );
  },
);

test("keeps first-time users in workspace setup until they continue explicitly", async () => {
  const user = userEvent.setup();
  const firstWorkspace = workspace();
  mocks.workspaces.set([]);
  mocks.browseWorkspaceRoot.mockResolvedValue({
    root_id: "root-1",
    path: "",
    canonical_path: "/repo",
    parent_path: null,
    entries: [{ name: "pontia", path: "pontia", kind: "directory", is_workspace: false }],
    warnings: [],
  });
  mocks.registerWorkspace.mockImplementation(async () => {
    mocks.workspaces.set([firstWorkspace]);
    return firstWorkspace;
  });

  render(NewChatPage);
  const continueButton = await screen.findByRole("button", { name: "Continue to New Chat" });
  expect(continueButton).toBeDisabled();

  await user.click(await screen.findByRole("button", { name: "Activate pontia" }));

  await waitFor(() =>
    expect(mocks.registerWorkspace).toHaveBeenCalledWith({
      root_id: "root-1",
      path: "pontia",
      name: "pontia",
    }),
  );
  expect(screen.getByRole("heading", { name: "Set up your first workspace" })).toBeInTheDocument();
  expect(screen.queryByPlaceholderText("Describe a task…")).not.toBeInTheDocument();
  expect(continueButton).toBeEnabled();

  await user.click(continueButton);

  expect(await screen.findByPlaceholderText("Describe a task…")).toBeInTheDocument();
  expect(
    screen.queryByRole("heading", { name: "Set up your first workspace" }),
  ).not.toBeInTheDocument();
});

test.each([true, false])(
  "waits for the latest workspace request before choosing setup, with registered workspaces: %s",
  async (hasWorkspace) => {
    let resolveWorkspaces!: (workspaces: ReturnType<typeof workspace>[]) => void;
    mocks.listWorkspaces.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveWorkspaces = resolve;
      }),
    );

    await act(async () => {
      render(NewChatPage);
    });

    expect(screen.queryByRole("button", { name: "Start session" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Continue to New Chat" })).not.toBeInTheDocument();

    resolveWorkspaces(hasWorkspace ? [workspace()] : []);

    if (hasWorkspace) {
      expect(await screen.findByPlaceholderText("Describe a task…")).toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Continue to New Chat" }),
      ).not.toBeInTheDocument();
    } else {
      expect(await screen.findByRole("button", { name: "Continue to New Chat" })).toBeDisabled();
    }
  },
);

test("explains how to configure Pontia when no workspace roots exist", async () => {
  mocks.workspaces.set([]);
  mocks.workspaceRoots.set([]);

  render(NewChatPage);

  expect(await screen.findByText("No workspace roots configured")).toBeInTheDocument();
  expect(screen.getByText("pontia init")).toBeInTheDocument();
  expect(screen.getByText(/restart Pontia/i)).toBeInTheDocument();
});

test("focuses the prompt only on the first entry to the new chat page", async () => {
  const firstPage = render(NewChatPage);

  const firstPrompt = await screen.findByPlaceholderText("Describe a task…");
  await waitFor(() => expect(firstPrompt).toHaveFocus());
  firstPage.unmount();

  render(NewChatPage);
  const revisitedPrompt = await screen.findByPlaceholderText("Describe a task…");
  expect(revisitedPrompt).not.toHaveFocus();
});

test("prefers the new chat workspace query parameter over the remembered workspace", async () => {
  window.history.pushState({}, "", "/dashboard?workspace=workspace-2");
  window.localStorage.setItem("pontia.chat.lastWorkspaceId:local", "workspace-1");
  mocks.workspaces.set([
    workspace({ workspace_id: "workspace-1", name: "pontia" }),
    workspace({
      workspace_id: "workspace-2",
      name: "sandbox",
      canonical_path: "/repo/sandbox",
      display_path: "~/repo/sandbox",
    }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText("Describe a task…");
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent("sandbox");
  expect(window.localStorage.getItem("pontia.chat.lastWorkspaceId:local")).toBe("workspace-1");
});

test("updates the selected workspace when the mounted page query changes", async () => {
  mocks.workspaces.set([
    workspace({ workspace_id: "workspace-1", name: "pontia" }),
    workspace({
      workspace_id: "workspace-2",
      name: "sandbox",
      canonical_path: "/repo/sandbox",
      display_path: "~/repo/sandbox",
    }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText("Describe a task…");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent("pontia");

  window.history.pushState({}, "", "/dashboard?workspace=workspace-2");
  window.dispatchEvent(new PopStateEvent("popstate"));

  await waitFor(() => expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent("sandbox"));
});

test("remembers the selected new chat workspace after starting a chat", async () => {
  const user = userEvent.setup();
  const created = session({ session_id: "session-selected-workspace" });
  mocks.createSession.mockResolvedValue({
    session: created,
    initial_turn: turn({ session_id: "session-selected-workspace" }),
  } satisfies CreateSessionResult);
  mocks.workspaces.set([
    workspace({ workspace_id: "workspace-1", name: "pontia" }),
    workspace({
      workspace_id: "workspace-2",
      name: "sandbox",
      canonical_path: "/repo/sandbox",
      display_path: "~/repo/sandbox",
    }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText("Describe a task…");
  const workspaceSelector = screen.getByLabelText(/^Workspace$/i);
  await user.click(workspaceSelector);
  await user.click(await screen.findByText("sandbox"));
  expect(workspaceSelector).toHaveTextContent("sandbox");
  document.body.style.pointerEvents = "";
  await user.type(screen.getByPlaceholderText("Describe a task…"), "Use sandbox");
  await user.click(screen.getByRole("button", { name: /start session/i }));

  await vi.waitFor(() =>
    expect(mocks.createSession).toHaveBeenCalledWith(
      expect.objectContaining({ workspace_id: "workspace-2" }),
    ),
  );
  expect(window.localStorage.getItem("pontia.chat.lastWorkspaceId:local")).toBe("workspace-2");
});

test("remembers the selected agent client for the next new chat", async () => {
  const user = userEvent.setup();
  const { unmount } = render(NewChatPage);

  const clientSelector = await screen.findByLabelText("Agent client");
  await user.click(clientSelector);
  await user.click(await screen.findByText("codex"));
  expect(clientSelector).toHaveTextContent("codex");
  expect(window.localStorage.getItem("pontia.chat.lastCreationOptionId:local")).toBe("codex");
  unmount();
  document.body.style.pointerEvents = "";

  render(NewChatPage);
  expect(await screen.findByLabelText("Agent client")).toHaveTextContent("codex");
});

test("switching clients clears the selected startup model", async () => {
  const user = userEvent.setup();
  const catalog = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [{ id: "pi-only", name: "Pi model", description: "" }],
    version: null,
    fetched_at: Date.now(),
    default_model: null,
  });
  try {
    render(NewChatPage);
    await user.click(await screen.findByRole("button", { name: "Client default model" }));
    await user.click(await screen.findByLabelText("Pi model"));
    await fireEvent.keyDown(window, { code: "KeyC", altKey: true });
    await waitFor(() => expect(screen.getByLabelText("Agent client")).toHaveTextContent("codex"));
    await fireEvent.keyDown(window, { code: "KeyC", altKey: true, shiftKey: true });
    await waitFor(() => expect(screen.getByLabelText("Agent client")).toHaveTextContent("Pi"));
    await user.type(screen.getByPlaceholderText("Describe a task…"), "Use the default");
    await user.click(screen.getByRole("button", { name: /start session/i }));
    await waitFor(() => expect(mocks.createSession).toHaveBeenCalledOnce());
    expect(mocks.createSession.mock.calls[0][0]).not.toHaveProperty("model");
  } finally {
    catalog.mockRestore();
  }
});

test("selects the client default model without sending it on creation", async () => {
  const user = userEvent.setup();
  const catalog = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [
      { id: "provider/other", name: "Other model", description: "" },
      { id: "provider/default", name: "Default model", description: "" },
    ],
    version: null,
    fetched_at: Date.now(),
    default_model: "provider/default",
  });
  try {
    render(NewChatPage);
    await waitFor(() =>
      expect(document.getElementById("session-model")).toHaveTextContent("Default model"),
    );
    await user.click(document.getElementById("session-model")!);
    const list = await screen.findByLabelText("Available models");
    expect(within(list).getByLabelText("Client default model")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(within(list).getByLabelText("Default model")).toHaveAttribute("aria-selected", "false");
    expect(within(list).getByText("Currently Default model")).toBeInTheDocument();
    await user.click(within(list).getByLabelText("Other model"));
    await waitFor(() =>
      expect(document.getElementById("session-model")).toHaveTextContent("Other model"),
    );
    document.body.style.pointerEvents = "";
    await user.click(document.getElementById("session-model")!);
    await user.click(await screen.findByLabelText("Client default model"));
    document.body.style.pointerEvents = "";
    await user.type(screen.getByPlaceholderText("Describe a task…"), "Use the default");
    await user.click(screen.getByRole("button", { name: /start session/i }));
    await waitFor(() => expect(mocks.createSession).toHaveBeenCalledOnce());
    expect(mocks.createSession.mock.calls[0][0]).not.toHaveProperty("model");
  } finally {
    catalog.mockRestore();
  }
});

test("resets the effort when the model changes and closes on an effort or an effortless model", async () => {
  const user = userEvent.setup();
  const high = { id: "high", name: "high", description: "" };
  const catalog = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [
      { id: "a", name: "First", description: "", reasoning_efforts: [high] },
      { id: "b", name: "Second", description: "", reasoning_efforts: [high] },
      { id: "c", name: "Third", description: "", reasoning_efforts: [] },
    ],
    version: null,
    fetched_at: Date.now(),
    default_model: "a",
  });
  try {
    render(NewChatPage);
    const trigger = () => document.getElementById("session-model")!;
    await waitFor(() => expect(trigger()).toHaveTextContent("First · Default"));
    const efforts = async () => screen.findByLabelText("Reasoning effort");
    await user.click(trigger());
    await user.click(within(await efforts()).getByLabelText("high"));
    await waitFor(() => expect(trigger()).toHaveTextContent("First · high"));

    document.body.style.pointerEvents = "";
    await user.click(trigger());
    await user.click(await screen.findByLabelText("Second"));
    expect(within(await efforts()).getByLabelText("Default")).toHaveAttribute(
      "aria-selected",
      "false",
    );
    await user.click(within(await efforts()).getByLabelText("Default"));
    await waitFor(() => expect(trigger()).toHaveTextContent("Second · Default"));

    document.body.style.pointerEvents = "";
    await user.click(trigger());
    await user.click(await screen.findByLabelText("Third"));
    await waitFor(() => expect(screen.queryByRole("listbox", { hidden: true })).toBeNull());
    expect(trigger()).toHaveTextContent(/^Third$/);
    document.body.style.pointerEvents = "";
    await user.type(screen.getByPlaceholderText("Describe a task…"), "Use the default");
    await user.click(screen.getByRole("button", { name: /start session/i }));
    await waitFor(() => expect(mocks.createSession).toHaveBeenCalledOnce());
    expect(mocks.createSession.mock.calls[0][0]).toMatchObject({ model: "c" });
    expect(mocks.createSession.mock.calls[0][0]).not.toHaveProperty("reasoning_effort");
  } finally {
    catalog.mockRestore();
  }
});

test("falls back to the first agent client when the remembered one is unavailable", async () => {
  window.localStorage.setItem("pontia.chat.lastCreationOptionId:local", "removed-client");
  render(NewChatPage);

  await waitFor(() => expect(screen.getByLabelText("Agent client")).toHaveTextContent("Pi"));
  expect(window.localStorage.getItem("pontia.chat.lastCreationOptionId:local")).toBe("pi");
});

test("offers Pi and Codex and requires a task before submission", async () => {
  render(NewChatPage);

  const clientSelector = await screen.findByLabelText("Agent client");
  expect(clientSelector).toHaveTextContent("Pi");
  await userEvent.click(clientSelector);
  expect(await screen.findByText("codex")).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  document.body.style.pointerEvents = "";
  expect(screen.getByRole("button", { name: "Start session" })).toBeDisabled();
  await userEvent.type(screen.getByRole("textbox"), "Inspect this workspace");
  expect(screen.getByRole("button", { name: "Start session" })).toBeEnabled();
});

test.each([true, false])(
  "recovers from a workspace load error, with registered workspaces: %s",
  async (hasWorkspace) => {
    mocks.listWorkspaces.mockRejectedValueOnce(new Error("Workspace request failed"));
    render(NewChatPage);

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalled());
    expect(screen.getByRole("button", { name: "Start session" })).toBeDisabled();
    mocks.listWorkspaces.mockResolvedValueOnce(hasWorkspace ? [workspace()] : []);
    const notification = mocks.toastError.mock.lastCall?.[1];
    notification.action.onClick();
    if (hasWorkspace) {
      await waitFor(() => expect(screen.getByLabelText("Workspace")).toHaveTextContent("pontia"));
    } else {
      expect(
        await screen.findByRole("heading", { name: "Set up your first workspace" }),
      ).toBeInTheDocument();
    }
  },
);

test("creates a session with initial prompt, workspace, and client then opens its chat", async () => {
  const user = userEvent.setup();
  const created = session({ session_id: "session-new" });
  mocks.createSession.mockResolvedValue({
    session: created,
    initial_turn: turn({ session_id: "session-new" }),
  } satisfies CreateSessionResult);
  const catalog = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [
      {
        id: "provider/model",
        name: "Selected model",
        description: "",
        reasoning_efforts: [{ id: "high", name: "high", description: "" }],
      },
    ],
    version: null,
    fetched_at: Date.now(),
    default_model: null,
  });
  render(NewChatPage);
  await user.type(
    await screen.findByPlaceholderText("Describe a task…"),
    "Implement the dashboard chat flow",
  );
  await user.click(await screen.findByRole("button", { name: "Client default model" }));
  await user.click(await screen.findByLabelText("Selected model"));
  const efforts = await screen.findByLabelText("Reasoning effort");
  await user.click(within(efforts).getByLabelText("high"));
  document.body.style.pointerEvents = "";
  catalog.mockRestore();
  await fireEvent.click(screen.getByRole("button", { name: /start session/i }));

  await waitFor(() =>
    expect(mocks.createSession).toHaveBeenCalledWith({
      client_type: "pi",
      workspace_id: "workspace-1",
      model: "provider/model",
      reasoning_effort: "high",
      title: "Implement the dashboard chat flow",
      include_standalone_session_instructions: true,
      initial_task: {
        input: "Implement the dashboard chat flow",
        metadata: { source: "dashboard_chat" },
      },
      metadata: { source: "dashboard_chat", title_source: "input" },
    }),
  );
  expect(mocks.navigate).toHaveBeenCalledWith("/chat/session-new");
});
