import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";
import type {
  AgentClientAuthProbe,
  AgentClientAuthStatus,
  AgentClientView,
} from "../src/api/types";
import AgentClientOnboardingDialog from "../src/features/onboarding/AgentClientOnboardingDialog.svelte";
import {
  agentClientOnboardingMarker,
  runAgentClientOnboardingProbe,
  shouldShowAgentClientOnboarding,
} from "../src/features/onboarding/agentClientOnboarding";

function client(id: string, overrides: Partial<AgentClientView> = {}): AgentClientView {
  return {
    id,
    client_type: "codex",
    client_key: "default",
    name: "Codex",
    short_id: "codex",
    enabled: true,
    detection: { status: "detected", detail: null },
    setup: { status: "not_required" },
    setup_command: null,
    install: {
      supported: true,
      provenance: "pontia_managed",
      available_actions: [],
      methods: [
        {
          id: "npm",
          label: "npm",
          recommended: true,
          supported_on_platform: true,
          source_summary: "npm registry",
          target_summary: "~/.local/bin",
          prerequisites: [],
          ready: true,
        },
      ],
      auth: {
        status: "not_configured",
        login_command: "codex login",
        headless_login_command: null,
        env_vars: [],
        docs_url: null,
      },
    },
    auth_supported: true,
    version: "1.0.0",
    version_checked_at: null,
    has_record: true,
    created_at: null,
    updated_at: null,
    ...overrides,
  };
}

function auth(
  state: AgentClientAuthStatus["state"],
  methods: AgentClientAuthStatus["methods"] = [],
): AgentClientAuthStatus {
  return {
    state,
    checked_at: "2026-11-27T00:00:00.000Z",
    methods,
    supports_logout: false,
  };
}

function probe(
  entries: AgentClientAuthProbe["agent_clients"],
  firstProbe = true,
): AgentClientAuthProbe {
  return { first_probe: firstProbe, agent_clients: entries };
}

describe("the device-scoped one-time probe", () => {
  beforeEach(() => localStorage.clear());

  test("writes the device marker before requesting and never requests that device again", async () => {
    const request = vi.fn(async () => {
      expect(localStorage.getItem(agentClientOnboardingMarker("device-a"))).not.toBeNull();
      return "result";
    });

    await expect(
      runAgentClientOnboardingProbe({
        deviceId: "device-a",
        storage: localStorage,
        probe: request,
        now: () => "2026-11-27T00:00:00.000Z",
      }),
    ).resolves.toEqual({ checked: true, result: "result" });
    await runAgentClientOnboardingProbe({
      deviceId: "device-a",
      storage: localStorage,
      probe: request,
    });

    expect(request).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(agentClientOnboardingMarker("device-a"))).toBe(
      "2026-11-27T00:00:00.000Z",
    );
  });

  test("marks and probes another device independently", async () => {
    const request = vi.fn().mockResolvedValue(undefined);
    localStorage.setItem(agentClientOnboardingMarker("device-a"), "already-checked");

    await runAgentClientOnboardingProbe({
      deviceId: "device-b",
      storage: localStorage,
      probe: request,
    });

    expect(request).toHaveBeenCalledOnce();
    expect(localStorage.getItem(agentClientOnboardingMarker("device-b"))).not.toBeNull();
  });

  test("keeps the marker when the request fails", async () => {
    await expect(
      runAgentClientOnboardingProbe({
        deviceId: "device-a",
        storage: localStorage,
        probe: () => Promise.reject(new Error("offline")),
      }),
    ).rejects.toThrow("offline");
    expect(localStorage.getItem(agentClientOnboardingMarker("device-a"))).not.toBeNull();
  });
});

describe("onboarding trigger conditions", () => {
  test("shows when no client is installed", () => {
    const missing = client("missing", {
      detection: { status: "not_detected", detail: null },
      enabled: false,
    });
    expect(shouldShowAgentClientOnboarding([missing], probe([]))).toBe(true);
  });

  test("shows when no installed client is enabled", () => {
    const disabled = client("disabled", { enabled: false });
    expect(
      shouldShowAgentClientOnboarding(
        [disabled],
        probe([{ client_type: "codex", client_key: "default", auth: auth("authenticated") }]),
      ),
    ).toBe(true);
  });

  test("shows when no installed client is signed in", () => {
    const installed = client("installed");
    expect(
      shouldShowAgentClientOnboarding(
        [installed],
        probe([{ client_type: "codex", client_key: "default", auth: auth("unauthenticated") }]),
      ),
    ).toBe(true);
  });

  test("treats unknown as not signed in", () => {
    const installed = client("installed");
    expect(
      shouldShowAgentClientOnboarding(
        [installed],
        probe([{ client_type: "codex", client_key: "default", auth: auth("unknown") }]),
      ),
    ).toBe(true);
  });

  test("never shows when the backend says this is not the first probe", () => {
    const disabled = client("disabled", { enabled: false });
    expect(shouldShowAgentClientOnboarding([disabled], probe([], false))).toBe(false);
  });

  test("does not show when installed, enabled, and signed-in clients all exist", () => {
    const installed = client("installed");
    expect(
      shouldShowAgentClientOnboarding(
        [installed],
        probe([{ client_type: "codex", client_key: "default", auth: auth("authenticated") }]),
      ),
    ).toBe(false);
  });
});

describe("the onboarding dialog", () => {
  function callbacks() {
    return {
      getAuth: vi.fn().mockResolvedValue(auth("unauthenticated")),
      startLogin: vi.fn(),
      getLogin: vi.fn(),
      streamLogin: vi.fn(),
      cancelLogin: vi.fn(),
      submitLoginInput: vi.fn(),
      enable: vi.fn(async (value: AgentClientView) => ({ ...value, enabled: true })),
      onSkip: vi.fn(),
      onManage: vi.fn(),
      onComplete: vi.fn(),
    };
  }

  test("the skip exit closes the guide and reports the skip", async () => {
    const user = userEvent.setup();
    const installed = client("installed", { enabled: false });
    const actions = callbacks();
    render(AgentClientOnboardingDialog, {
      open: true,
      deviceName: "My Device",
      clients: [installed],
      initialAuth: [{ client: installed, auth: auth("unauthenticated") }],
      install: vi.fn(),
      ...actions,
    });

    await user.click(screen.getByRole("button", { name: "I'm configured, skip" }));

    expect(actions.onSkip).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  test("completes install, terminal sign-in, and enable in order", async () => {
    const user = userEvent.setup();
    const missing = client("missing", {
      enabled: false,
      detection: { status: "not_detected", detail: null },
      version: null,
    });
    const installed = {
      ...missing,
      detection: { status: "detected", detail: null } as const,
      version: "1.2.3",
    };
    const actions = callbacks();
    const install = vi.fn().mockResolvedValue(installed);
    render(AgentClientOnboardingDialog, {
      open: true,
      deviceName: "My Device",
      clients: [missing],
      initialAuth: [],
      install,
      ...actions,
    });

    await user.click(screen.getByRole("button", { name: "Install" }));
    expect(screen.getByRole("heading", { name: "Set up Codex" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(actions.getAuth).toHaveBeenCalledWith(installed));
    await user.click(await screen.findByRole("button", { name: "I signed in from the terminal" }));
    await user.click(await screen.findByRole("button", { name: "Enable" }));

    expect(install).toHaveBeenCalledOnce();
    expect(actions.enable).toHaveBeenCalledWith(installed);
    await user.click(await screen.findByRole("button", { name: "Start a chat" }));
    expect(actions.onComplete).toHaveBeenCalledOnce();
  });

  test("completes a managed sign-in from its status stream", async () => {
    const user = userEvent.setup();
    const installed = client("managed", { enabled: false });
    const method = {
      id: "device_code",
      label: "Account",
      kind: "device_code" as const,
      recommended: true,
      remote_compatible: true,
      start_fields: [
        {
          name: "region",
          label: "Region",
          kind: { type: "text" as const },
          required: true,
          default_value: "us-east-1",
        },
      ],
    };
    const starting = {
      session_id: "login-1",
      client_type: "codex",
      client_key: "default",
      method_id: method.id,
      state: "starting" as const,
      step: { type: "pending" as const },
      sequence: 1,
      created_at: "2026-11-27T00:00:00Z",
      updated_at: "2026-11-27T00:00:00Z",
      expires_at: "2026-11-27T00:10:00Z",
    };
    const succeeded = {
      ...starting,
      state: "succeeded" as const,
      sequence: 2,
      step: { type: "succeeded" as const, status: auth("authenticated", [method]) },
    };
    const encoder = new TextEncoder();
    const actions = callbacks();
    actions.startLogin.mockResolvedValue(starting);
    actions.streamLogin.mockResolvedValue(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(
              encoder.encode(`event: terminal\ndata: ${JSON.stringify(succeeded)}\n\n`),
            );
            controller.close();
          },
        }),
      ),
    );
    render(AgentClientOnboardingDialog, {
      open: true,
      deviceName: "My Device",
      clients: [installed],
      initialAuth: [{ client: installed, auth: auth("unauthenticated", [method]) }],
      install: vi.fn(),
      ...actions,
    });

    await user.click(screen.getByRole("button", { name: "Sign in" }));
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(actions.startLogin).toHaveBeenCalledWith(installed, method.id, {
        region: "us-east-1",
      }),
    );
    expect(await screen.findByRole("button", { name: "Enable" })).toBeEnabled();
  });

  test("a managed login finishing after navigation stays with its original client", async () => {
    const user = userEvent.setup();
    const first = client("first", { enabled: false, name: "First" });
    const second = client("second", {
      client_type: "pi",
      name: "Second",
      enabled: false,
    });
    const method = {
      id: "device_code",
      label: "Account",
      kind: "device_code" as const,
      recommended: true,
      remote_compatible: true,
      start_fields: [],
    };
    const starting = {
      session_id: "login-1",
      client_type: first.client_type,
      client_key: first.client_key,
      method_id: method.id,
      state: "starting" as const,
      step: { type: "pending" as const },
      sequence: 1,
      created_at: "2026-11-27T00:00:00Z",
      updated_at: "2026-11-27T00:00:00Z",
      expires_at: "2026-11-27T00:10:00Z",
    };
    let finishStream!: (response: Response) => void;
    const actions = callbacks();
    actions.startLogin.mockResolvedValue(starting);
    actions.getLogin.mockResolvedValue({
      ...starting,
      state: "cancelled",
      sequence: 2,
      step: { type: "cancelled" },
    });
    actions.streamLogin.mockImplementation(
      () => new Promise<Response>((resolve) => (finishStream = resolve)),
    );
    render(AgentClientOnboardingDialog, {
      open: true,
      deviceName: "My Device",
      clients: [first, second],
      initialAuth: [
        { client: first, auth: auth("unauthenticated", [method]) },
        { client: second, auth: auth("unauthenticated") },
      ],
      install: vi.fn(),
      ...actions,
    });

    await user.click(screen.getAllByRole("button", { name: "Sign in" })[0]);
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    await user.click(screen.getByRole("button", { name: "Back" }));
    await user.click(screen.getAllByRole("button", { name: "Sign in" })[1]);
    finishStream(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.close();
          },
        }),
      ),
    );

    expect(screen.getByRole("heading", { name: "Set up Second" })).toBeVisible();
    await waitFor(() => expect(actions.getLogin).toHaveBeenCalledWith(first, "login-1"));
  });
});
