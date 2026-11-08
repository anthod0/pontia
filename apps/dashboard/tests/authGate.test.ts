import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import { writable } from "svelte/store";
import AppLayoutHost from "./components/AppLayoutHost.svelte";
import AppLayoutRequestHost from "./components/AppLayoutRequestHost.svelte";
import { token } from "../src/stores/auth";

const mocks = vi.hoisted(() => ({
  startEventStream: vi.fn(),
  stopEventStream: vi.fn(),
  loadWorkspaces: vi.fn(async () => undefined),
  loadWorkflows: vi.fn(async () => undefined),
}));

vi.mock("../src/services/eventStream", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    startEventStream: mocks.startEventStream,
    stopEventStream: mocks.stopEventStream,
  };
  return {
    ...original,
    ...mocked,
    createEvents: (device) => ({ ...original.createEvents(device), ...mocked }),
  };
});
vi.mock("../src/stores/sessions", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    sessionDetail: writable(null),
    sessionDetailError: writable(null),
  };
  return {
    ...original,
    ...mocked,
    createSessions: (device) => ({ ...original.createSessions(device), ...mocked }),
  };
});
vi.mock("../src/queries/sessionOverview", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    createSessionOverviewQuery: () => ({
      data: undefined,
      isPending: false,
      isFetchingNextPage: false,
      error: null,
      fetchNextPage: vi.fn(),
    }),
    snapshotSessionOverview: () => ({
      pinned: [],
      active: [],
      list: [],
      archived: [],
      nextCursor: null,
    }),
  };
  return {
    ...original,
    ...mocked,
    createSessionOverview: (device) => ({ ...original.createSessionOverview(device), ...mocked }),
  };
});
vi.mock("../src/queries/workspaces", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    fetchWorkspaces: mocks.loadWorkspaces,
    createWorkspacesQuery: () => ({ data: [], isPending: false, error: null }),
  };
  return {
    ...original,
    ...mocked,
    createWorkspaceQueries: (device) => ({ ...original.createWorkspaceQueries(device), ...mocked }),
  };
});
vi.mock("../src/stores/workflows", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    resetWorkflows: vi.fn(),
  };
  return {
    ...original,
    ...mocked,
    createWorkflows: (device) => ({ ...original.createWorkflows(device), ...mocked }),
  };
});
vi.mock("../src/queries/workflows", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    fetchWorkflows: mocks.loadWorkflows,
  };
  return {
    ...original,
    ...mocked,
    createWorkflowQueries: (device) => ({ ...original.createWorkflowQueries(device), ...mocked }),
  };
});

beforeEach(() => {
  window.history.pushState({}, "", "/dashboard/tasks");
  localStorage.clear();
  token.set("");
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

function mockValidateToken(status: number): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(
    async () =>
      new Response(
        status === 200
          ? JSON.stringify({ data: { authenticated: true }, error: null })
          : JSON.stringify({
              data: null,
              error: { code: "authentication_failed", message: "missing or invalid bearer token" },
            }),
        { status },
      ),
  );
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

test("blocks dashboard routes behind a token prompt when no token is saved", async () => {
  render(AppLayoutHost);

  expect(screen.getByRole("heading", { name: /enter external api token/i })).toBeInTheDocument();
  expect(screen.getByLabelText(/bearer token/i)).toBeInTheDocument();
  expect(screen.queryByText("PONTIA")).not.toBeInTheDocument();
  expect(mocks.loadWorkspaces).not.toHaveBeenCalled();
  expect(mocks.startEventStream).not.toHaveBeenCalled();
});

test("stores a token from the URL and removes only that query parameter", async () => {
  window.history.pushState({}, "", "/dashboard/tasks?token=url-token&phase=review#activity");

  render(AppLayoutHost);

  await waitFor(() => expect(localStorage.getItem("pontia.externalApiToken")).toBe("url-token"));
  expect(`${window.location.pathname}${window.location.search}${window.location.hash}`).toBe(
    "/dashboard/tasks?phase=review#activity",
  );
  expect(
    screen.queryByRole("heading", { name: /enter external api token/i }),
  ).not.toBeInTheDocument();
  expect(mocks.startEventStream).toHaveBeenCalled();
});

test("uses the URL credential for initial route requests even when storage has a stale token", async () => {
  localStorage.setItem("pontia.externalApiToken", "stale-token");
  window.history.pushState({}, "", "/dashboard?token=current-token");
  const authorization: (string | null)[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_url: string, init: RequestInit) => {
      const bearer = new Headers(init.headers).get("Authorization");
      authorization.push(bearer);
      return new Response(JSON.stringify({ data: { options: [] }, error: null }), {
        status: bearer === "Bearer current-token" ? 200 : 401,
      });
    }),
  );

  render(AppLayoutRequestHost);

  await screen.findByTestId("authenticated-options");
  expect(authorization.length).toBeGreaterThan(0);
  expect(authorization.every((value) => value === "Bearer current-token")).toBe(true);
  expect(localStorage.getItem("pontia.externalApiToken")).toBe("current-token");
  expect(screen.queryByTestId("authentication-failed")).not.toBeInTheDocument();
});

test("removes an empty URL token without replacing a saved token", async () => {
  localStorage.setItem("pontia.externalApiToken", "saved-token");
  token.set("saved-token");
  window.history.pushState({}, "", "/dashboard/tasks?token=&phase=review");

  render(AppLayoutHost);

  await waitFor(() => expect(window.location.search).toBe("?phase=review"));
  expect(localStorage.getItem("pontia.externalApiToken")).toBe("saved-token");
  expect(
    screen.queryByRole("heading", { name: /enter external api token/i }),
  ).not.toBeInTheDocument();
});

test("rejects an invalid entered token without opening the dashboard", async () => {
  const fetchMock = mockValidateToken(401);
  render(AppLayoutHost);

  await fireEvent.input(screen.getByLabelText(/bearer token/i), {
    target: { value: " wrong-token " },
  });
  await fireEvent.click(screen.getByRole("button", { name: /continue/i }));

  await waitFor(() =>
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/auth/validate",
      expect.objectContaining({
        headers: expect.any(Headers),
      }),
    ),
  );
  const headers = fetchMock.mock.calls[0][1]?.headers as Headers;
  expect(headers.get("Authorization")).toBe("Bearer wrong-token");
  expect(localStorage.getItem("pontia.externalApiToken")).not.toBe("wrong-token");
  expect(screen.getByRole("heading", { name: /enter external api token/i })).toBeInTheDocument();
  expect(await screen.findByText(/invalid token/i)).toBeInTheDocument();
  expect(screen.queryByText("PONTIA")).not.toBeInTheDocument();
  expect(mocks.startEventStream).not.toHaveBeenCalled();
});

test("saves the entered token and opens the requested dashboard route after validation succeeds", async () => {
  mockValidateToken(200);
  render(AppLayoutHost);

  await fireEvent.input(screen.getByLabelText(/bearer token/i), {
    target: { value: " dev-token " },
  });
  await fireEvent.click(screen.getByRole("button", { name: /continue/i }));

  await waitFor(() => expect(localStorage.getItem("pontia.externalApiToken")).toBe("dev-token"));
  await waitFor(() =>
    expect(
      screen.queryByRole("heading", { name: /enter external api token/i }),
    ).not.toBeInTheDocument(),
  );
  expect(screen.getByText("Pontia")).toBeInTheDocument();
  expect(mocks.loadWorkspaces).toHaveBeenCalled();
  expect(mocks.startEventStream).toHaveBeenCalled();
});

test("opens dashboard immediately when a saved token exists without startup validation", async () => {
  localStorage.setItem("pontia.externalApiToken", "saved-token");
  token.set("saved-token");
  const fetchMock = mockValidateToken(401);

  render(AppLayoutHost);

  expect(
    screen.queryByRole("heading", { name: /enter external api token/i }),
  ).not.toBeInTheDocument();
  expect(screen.getByText("Pontia")).toBeInTheDocument();
  expect(localStorage.getItem("pontia.externalApiToken")).toBe("saved-token");
  expect(fetchMock).not.toHaveBeenCalled();
  expect(mocks.startEventStream).toHaveBeenCalled();
});
