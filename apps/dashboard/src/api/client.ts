import type { Locale } from "$lib/i18n";
import { ApiError } from "./errors";

import type {
  Interaction,
  ForkSessionOutcome,
  AgentClientModels,
  AgentClientView,
  AgentClientSetupOperation,
  Bot,
  BotMessage,
  BotMessagePage,
  BotMessageContext,
  BotMessageSearchPage,
  BotClient,
  BotConfiguration,
  BotDeletionBlockers,
  AgentProfileView,
  ApiEnvelope,
  CreateSessionInput,
  CreateSessionResult,
  EventView,
  InboxMessageView,
  RegisterWorkspaceInput,
  RenameWorkspaceInput,
  SessionView,
  SessionCompactionAccepted,
  SessionCreationOption,
  SessionOverviewSection,
  SessionOverviewView,
  SubmitInboxMessageInput,
  TurnTimelineDirection,
  TurnTimelinePage,
  TurnTreeHistoryPage,
  TurnTreeUpdatesPage,
  UpsertAgentProfileInput,
  TurnView,
  UpdateSessionInput,
  WorkspaceDirectoryListingView,
  FilePickerResultView,
  WorkspaceGitStatusView,
  GitChangeGroup,
  GitBranchesView,
  GitChangesView,
  GitCommitDiffView,
  GitCommitsPage,
  GitCommitView,
  GitDiffView,
  WorkspaceRootView,
  WorkspaceView,
  WorkflowDetailView,
  WorkflowPatchHistoryView,
  WorkflowDocumentView,
  WorkflowGraphRevisionView,
  WorkflowListItemView,
  MemoryListQuery,
  MemoryPage,
  UserFileView,
  ReplaceUserFileInput,
} from "./types";
import type { DeviceRuntime } from "../device/runtime";

type RequestOptions = Omit<RequestInit, "body"> & {
  body?: unknown;
  mutating?: boolean;
  retryNetworkErrors?: boolean;
  directResponse?: boolean;
  idempotency?: boolean;
};

export type ReadRequestOptions = Pick<RequestOptions, "signal">;

export function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

export function isTransientNetworkError(error: unknown): boolean {
  if (isAbortError(error)) return false;
  if (error instanceof TypeError) return true;
  if (error instanceof DOMException && error.name === "NetworkError") return true;

  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return /Failed to fetch|NetworkError|ERR_NETWORK_CHANGED|ERR_INTERNET_DISCONNECTED|ERR_NETWORK_ACCESS_DENIED|Load failed/i.test(
    message,
  );
}

export function isAuthenticationFailure(status: number): boolean {
  return status === 401 || status === 403;
}

export type GetSessionOverviewOptions = {
  sections: SessionOverviewSection[];
  workspaceId?: string;
  limit?: number;
  cursor?: string;
};
export function createApi(device: DeviceRuntime) {
  const API_BASE = "/api/v1";

  function botPath(id: string): string {
    return `/bots/${encodeURIComponent(id)}`;
  }

  async function listBots(signal?: AbortSignal): Promise<Bot[]> {
    return (await request<{ bots: Bot[] }>("/bots", { signal })).bots;
  }

  async function listBotClients(signal?: AbortSignal): Promise<BotClient[]> {
    return (await request<{ clients: BotClient[] }>("/bots/clients", { signal })).clients;
  }

  async function listAgentClientModels(
    clientType: string,
    clientKey: string,
    signal?: AbortSignal,
    workspaceId?: string,
  ): Promise<AgentClientModels> {
    const query = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : "";
    return request(
      `/agent-clients/${encodeURIComponent(clientType)}/${encodeURIComponent(clientKey)}/models${query}`,
      { signal },
    );
  }

  function getBotMessages(
    id: string,
    page: { before?: string; around?: string; limit?: number },
    signal?: AbortSignal,
  ): Promise<BotMessagePage> {
    const query = new URLSearchParams({ ...page, limit: String(page.limit ?? 50) });
    return request(`${botPath(id)}/messages?${query}`, { signal });
  }

  function searchBotMessages(
    id: string,
    page: { q: string; before?: string },
    signal?: AbortSignal,
  ): Promise<BotMessageSearchPage> {
    const query = new URLSearchParams({ q: page.q });
    if (page.before) query.set("before", page.before);
    return request(`${botPath(id)}/messages/search?${query}`, { signal });
  }

  function getBotTodos(
    id: string,
    signal?: AbortSignal,
  ): Promise<{ open: BotMessage[]; recently_resolved: BotMessage[] }> {
    return request(`${botPath(id)}/todos`, { signal });
  }

  async function changeBotTodo(
    id: string,
    messageId: string,
    action: "resolve" | "reopen",
  ): Promise<BotMessage> {
    return (
      await request<{ message: BotMessage }>(
        `${botPath(id)}/todos/${encodeURIComponent(messageId)}/${action}`,
        {
          method: "POST",
          retryNetworkErrors: false,
        },
      )
    ).message;
  }

  function getBotMessageContext(
    id: string,
    messageId: string,
    signal?: AbortSignal,
  ): Promise<BotMessageContext> {
    return request(`${botPath(id)}/messages/${encodeURIComponent(messageId)}/context`, { signal });
  }

  async function saveBot(input: BotConfiguration, id?: string, locale?: Locale): Promise<Bot> {
    const path = id ? botPath(id) : "/bots";
    return (
      await request<{ bot: Bot }>(path, {
        method: id ? "PUT" : "POST",
        body: id ? input : { ...input, locale },
        retryNetworkErrors: false,
      })
    ).bot;
  }

  function getBotDeletionBlockers(id: string, signal?: AbortSignal): Promise<BotDeletionBlockers> {
    return request(`${botPath(id)}/deletion-blockers`, { signal });
  }

  async function deleteBot(id: string): Promise<void> {
    await request(botPath(id), { method: "DELETE", idempotency: false, retryNetworkErrors: false });
  }

  type SentBotMessage = BotMessage & { delivery?: "accepted" | "queued" };

  async function sendBotMessage(id: string, content: string): Promise<SentBotMessage> {
    return (
      await request<{ message: SentBotMessage }>(`${botPath(id)}/messages`, {
        method: "POST",
        body: { content },
        retryNetworkErrors: false,
      })
    ).message;
  }

  async function retryBotMessage(id: string, messageId: string): Promise<SentBotMessage> {
    return (
      await request<{ message: SentBotMessage }>(
        `${botPath(id)}/messages/${encodeURIComponent(messageId)}/retry`,
        {
          method: "POST",
          retryNetworkErrors: false,
        },
      )
    ).message;
  }

  function agentClientPath(target: Pick<AgentClientView, "client_type" | "client_key">): string {
    return `/agent-clients/${encodeURIComponent(target.client_type)}/${encodeURIComponent(target.client_key)}`;
  }

  async function listAgentClients(): Promise<AgentClientView[]> {
    return (await request<{ agent_clients: AgentClientView[] }>("/agent-clients")).agent_clients;
  }

  async function patchAgentClient(
    target: Pick<AgentClientView, "client_type" | "client_key">,
    settings: { enabled: boolean },
  ): Promise<AgentClientView> {
    return (
      await request<{ agent_client: AgentClientView }>(agentClientPath(target), {
        method: "PATCH",
        body: settings,
        mutating: true,
        idempotency: false,
        retryNetworkErrors: false,
      })
    ).agent_client;
  }

  async function resetAgentClient(target: AgentClientView): Promise<void> {
    await request(agentClientPath(target), {
      method: "DELETE",
      mutating: true,
      idempotency: false,
      retryNetworkErrors: false,
    });
  }

  async function setupAgentClient(target: AgentClientView): Promise<AgentClientSetupOperation> {
    return (
      await request<{ operation: AgentClientSetupOperation }>(`${agentClientPath(target)}/setup`, {
        method: "POST",
        mutating: true,
        idempotency: false,
        retryNetworkErrors: false,
      })
    ).operation;
  }

  async function getAgentClientSetup(
    options: ReadRequestOptions = {},
  ): Promise<AgentClientSetupOperation | null> {
    return (
      await request<{ operation: AgentClientSetupOperation | null }>("/agent-client-setup", options)
    ).operation;
  }

  async function listSessionCreationOptions(
    options: RequestOptions = {},
  ): Promise<SessionCreationOption[]> {
    return (
      await request<{ options: SessionCreationOption[] }>("/session-creation-options", options)
    ).options;
  }

  const TRANSIENT_NETWORK_RETRY_DELAYS_MS = [250, 750, 1500];

  function idempotencyKey(): string {
    return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  function delay(ms: number, signal?: AbortSignal | null): Promise<void> {
    if (signal?.aborted)
      return Promise.reject(new DOMException("The operation was aborted.", "AbortError"));
    return new Promise((resolve, reject) => {
      const cleanup = () => signal?.removeEventListener("abort", abort);
      const timeout = window.setTimeout(() => {
        cleanup();
        resolve();
      }, ms);
      const abort = () => {
        window.clearTimeout(timeout);
        cleanup();
        reject(new DOMException("The operation was aborted.", "AbortError"));
      };
      signal?.addEventListener("abort", abort, { once: true });
    });
  }

  async function fetchWithTransientNetworkRetry(
    input: RequestInfo | URL,
    init: RequestInit,
    onNetworkFailure?: () => void,
  ): Promise<Response> {
    let attempt = 0;
    while (true) {
      try {
        return await device.access.apiFetch(String(input), init);
      } catch (error) {
        if (!isTransientNetworkError(error) || attempt >= TRANSIENT_NETWORK_RETRY_DELAYS_MS.length)
          throw error;
        onNetworkFailure?.();
        await delay(TRANSIENT_NETWORK_RETRY_DELAYS_MS[attempt], init.signal);
        attempt += 1;
      }
    }
  }

  async function listPendingInteractions(session: string): Promise<Interaction[]> {
    const interactions: Interaction[] = [];
    let cursor: string | null = null;
    do {
      const query = new URLSearchParams({ state: "pending" });
      if (cursor) query.set("cursor", cursor);
      const page = await request<{ interactions: Interaction[]; next_cursor: string | null }>(
        `/sessions/${encodeURIComponent(session)}/interactions?${query}`,
      );
      interactions.push(...page.interactions);
      cursor = page.next_cursor;
    } while (cursor);
    return interactions;
  }

  async function getInteraction(session: string, id: string): Promise<Interaction> {
    return (
      await request<{ interaction: Interaction }>(
        `/sessions/${encodeURIComponent(session)}/interactions/${encodeURIComponent(id)}`,
      )
    ).interaction;
  }

  async function respondInteraction(
    session: string,
    id: string,
    option_key: string,
  ): Promise<Interaction> {
    return (
      await request<{ interaction: Interaction }>(
        `/sessions/${encodeURIComponent(session)}/interactions/${encodeURIComponent(id)}/response`,
        {
          method: "PUT",
          body: { option_key },
          idempotency: false,
          retryNetworkErrors: false,
        },
      )
    ).interaction;
  }

  async function validateExternalApiToken(candidateToken: string): Promise<void> {
    const headers = new Headers();
    headers.set("Authorization", `Bearer ${candidateToken}`);
    const response = await fetchWithTransientNetworkRetry(
      device.access.apiUrl(`${API_BASE}/auth/validate`),
      {
        headers,
        credentials: device.access.apiCredentials,
        signal: device.access.apiSignal(),
      },
    );
    const text = await response.text();
    let envelope: ApiEnvelope<unknown> | null = null;
    try {
      envelope = text ? (JSON.parse(text) as ApiEnvelope<unknown>) : null;
    } catch {
      throw new ApiError(text || response.statusText, "invalid_json", response.status);
    }
    if (!response.ok || envelope?.error) {
      throw new ApiError(
        envelope?.error?.message ?? response.statusText,
        envelope?.error?.code ?? "request_failed",
        response.status,
      );
    }
  }

  async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
    const {
      body,
      mutating,
      retryNetworkErrors,
      directResponse,
      idempotency = true,
      ...fetchOptions
    } = options;
    const headers = new Headers(fetchOptions.headers);
    device.access.applyApiAuthentication(headers);
    if (body !== undefined) headers.set("Content-Type", "application/json");
    if (
      idempotency &&
      (mutating || (fetchOptions.method && fetchOptions.method !== "GET")) &&
      !headers.has("Idempotency-Key")
    )
      headers.set("Idempotency-Key", idempotencyKey());

    let afterNetworkFailure = false;
    const fetchRequest =
      retryNetworkErrors === false
        ? (input: RequestInfo | URL, init: RequestInit) =>
            device.access.apiFetch(String(input), init)
        : (input: RequestInfo | URL, init: RequestInit) =>
            fetchWithTransientNetworkRetry(input, init, () => {
              afterNetworkFailure = true;
            });
    const response = await fetchRequest(device.access.apiUrl(`${API_BASE}${path}`), {
      ...fetchOptions,
      headers,
      credentials: device.access.apiCredentials,
      signal: device.access.apiSignal(fetchOptions.signal),
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    let envelope: ApiEnvelope<T> | null = null;
    try {
      envelope = text ? (JSON.parse(text) as ApiEnvelope<T>) : null;
    } catch {
      throw new ApiError(text || response.statusText, "invalid_json", response.status);
    }
    if (!response.ok || envelope?.error) {
      if (isAuthenticationFailure(response.status)) device.access.handleApiAuthenticationFailure();
      throw new ApiError(
        envelope?.error?.message ?? response.statusText,
        envelope?.error?.code ?? "request_failed",
        response.status,
        afterNetworkFailure,
      );
    }
    if (directResponse) return envelope as T;
    if (!envelope || envelope.data === null) {
      throw new ApiError("Response did not include data.", "missing_data", response.status);
    }
    return envelope.data;
  }

  // Snapshot consumers coalesce refreshes. Bound reads so a stalled connection
  // cannot hold their in-flight slot forever, including while reading the body.
  function boundedReadRequest<T>(path: string, options: ReadRequestOptions): Promise<T> {
    const timeout = AbortSignal.timeout(15_000);
    const signal = options.signal ? AbortSignal.any([options.signal, timeout]) : timeout;
    return request<T>(path, { ...options, signal });
  }

  async function getUserFile(options: ReadRequestOptions = {}): Promise<UserFileView> {
    return (await boundedReadRequest<{ user: UserFileView }>("/memory/user", options)).user;
  }

  async function replaceUserFile(input: ReplaceUserFileInput): Promise<UserFileView> {
    return (await request<{ user: UserFileView }>("/memory/user", { method: "PUT", body: input }))
      .user;
  }

  async function listMemories(
    query: MemoryListQuery,
    options: ReadRequestOptions = {},
  ): Promise<MemoryPage> {
    const params = new URLSearchParams();
    if (query.q) params.set("q", query.q);
    if (query.cursor) params.set("cursor", query.cursor);
    const search = params.toString();
    return boundedReadRequest<MemoryPage>(`/memories${search ? `?${search}` : ""}`, options);
  }

  async function listAgentProfiles(options: ReadRequestOptions = {}): Promise<AgentProfileView[]> {
    return (
      await boundedReadRequest<{ agent_profiles: AgentProfileView[] }>("/agent-profiles", options)
    ).agent_profiles;
  }

  async function getAgentProfile(
    profileId: string,
    options: ReadRequestOptions = {},
  ): Promise<AgentProfileView> {
    return (
      await boundedReadRequest<{ agent_profile: AgentProfileView }>(
        `/agent-profiles/${encodeURIComponent(profileId)}`,
        options,
      )
    ).agent_profile;
  }

  async function createAgentProfile(input: UpsertAgentProfileInput): Promise<AgentProfileView> {
    return (
      await request<{ agent_profile: AgentProfileView }>("/agent-profiles", {
        method: "POST",
        body: input,
        mutating: true,
      })
    ).agent_profile;
  }

  async function updateAgentProfile(
    profileId: string,
    input: UpsertAgentProfileInput,
  ): Promise<AgentProfileView> {
    return (
      await request<{ agent_profile: AgentProfileView }>(
        `/agent-profiles/${encodeURIComponent(profileId)}`,
        {
          method: "PUT",
          body: input,
          mutating: true,
        },
      )
    ).agent_profile;
  }

  async function deleteAgentProfile(profileId: string): Promise<{ profile_id: string }> {
    return request<{ profile_id: string }>(`/agent-profiles/${encodeURIComponent(profileId)}`, {
      method: "DELETE",
      mutating: true,
    });
  }

  async function listWorkflows(
    limit = 50,
    options: ReadRequestOptions = {},
  ): Promise<WorkflowListItemView[]> {
    return (
      await boundedReadRequest<{ workflows: WorkflowListItemView[] }>(
        `/workflows?limit=${limit}`,
        options,
      )
    ).workflows;
  }

  async function getWorkflow(
    workflowId: string,
    options: ReadRequestOptions = {},
  ): Promise<WorkflowDetailView> {
    return (
      await boundedReadRequest<{ workflow: WorkflowDetailView }>(
        `/workflows/${encodeURIComponent(workflowId)}`,
        options,
      )
    ).workflow;
  }

  async function getWorkflowRevision(
    workflowId: string,
    revision: number,
    options: ReadRequestOptions = {},
  ): Promise<WorkflowGraphRevisionView> {
    return (
      await request<{ revision: WorkflowGraphRevisionView }>(
        `/workflows/${encodeURIComponent(workflowId)}/revisions/${revision}`,
        options,
      )
    ).revision;
  }

  async function listWorkflowPatches(
    workflowId: string,
    options: ReadRequestOptions = {},
  ): Promise<WorkflowPatchHistoryView[]> {
    return (
      await boundedReadRequest<{ patches: WorkflowPatchHistoryView[] }>(
        `/workflows/${encodeURIComponent(workflowId)}/patches`,
        options,
      )
    ).patches;
  }

  async function getWorkflowDocument(
    workflowId: string,
    ref: string,
    options: ReadRequestOptions = {},
  ): Promise<WorkflowDocumentView> {
    const query = new URLSearchParams({ ref });
    return (
      await boundedReadRequest<{ document: WorkflowDocumentView }>(
        `/workflows/${encodeURIComponent(workflowId)}/documents?${query}`,
        options,
      )
    ).document;
  }

  async function pauseWorkflow(workflowId: string): Promise<WorkflowDetailView> {
    return (
      await request<{ workflow: WorkflowDetailView }>(
        `/workflows/${encodeURIComponent(workflowId)}/pause`,
        { method: "POST", mutating: true },
      )
    ).workflow;
  }

  async function resumeWorkflow(workflowId: string): Promise<WorkflowDetailView> {
    return (
      await request<{ workflow: WorkflowDetailView }>(
        `/workflows/${encodeURIComponent(workflowId)}/resume`,
        { method: "POST", mutating: true },
      )
    ).workflow;
  }

  async function retryWorkflow(
    workflowId: string,
    failureEventId: string,
  ): Promise<WorkflowDetailView> {
    return (
      await request<{ workflow: WorkflowDetailView }>(
        `/workflows/${encodeURIComponent(workflowId)}/retry`,
        {
          method: "POST",
          mutating: true,
          body: { failure_event_id: failureEventId },
        },
      )
    ).workflow;
  }

  async function getSessionOverview(
    options: GetSessionOverviewOptions,
    readOptions: ReadRequestOptions = {},
  ): Promise<SessionOverviewView> {
    const query = new URLSearchParams({ sections: options.sections.join(",") });
    if (options.workspaceId !== undefined) query.set("workspace_id", options.workspaceId);
    if (options.limit !== undefined) query.set("limit", String(options.limit));
    if (options.cursor !== undefined) query.set("cursor", options.cursor);
    const timeout = AbortSignal.timeout(15_000);
    const signal = readOptions.signal ? AbortSignal.any([readOptions.signal, timeout]) : timeout;
    return request<SessionOverviewView>(`/sessions/overview?${query}`, {
      signal,
      directResponse: true,
    });
  }

  async function listWorkspaces(options: ReadRequestOptions = {}): Promise<WorkspaceView[]> {
    return (await boundedReadRequest<{ workspaces: WorkspaceView[] }>("/workspaces", options))
      .workspaces;
  }

  async function registerWorkspace(input: RegisterWorkspaceInput): Promise<WorkspaceView> {
    return (
      await request<{ workspace: WorkspaceView }>("/workspaces", {
        method: "POST",
        body: input,
        mutating: true,
      })
    ).workspace;
  }

  async function renameWorkspace(
    workspaceId: string,
    input: RenameWorkspaceInput,
  ): Promise<WorkspaceView> {
    return (
      await request<{ workspace: WorkspaceView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}`,
        {
          method: "PATCH",
          body: input,
          mutating: true,
        },
      )
    ).workspace;
  }

  async function deleteWorkspace(workspaceId: string): Promise<WorkspaceView> {
    return (
      await request<{ workspace: WorkspaceView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}`,
        {
          method: "DELETE",
          mutating: true,
        },
      )
    ).workspace;
  }

  async function refreshWorkspaceGitStatus(workspaceId: string): Promise<WorkspaceGitStatusView> {
    return (
      await request<{ git_status: WorkspaceGitStatusView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git-status/refresh`,
        { method: "POST", mutating: true },
      )
    ).git_status;
  }

  async function getWorkspaceGitChanges(
    workspaceId: string,
    options: ReadRequestOptions = {},
  ): Promise<GitChangesView> {
    return (
      await request<{ changes: GitChangesView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/changes`,
        options,
      )
    ).changes;
  }

  async function getWorkspaceGitDiff(
    workspaceId: string,
    file: { path: string; orig_path?: string; group: GitChangeGroup },
    options: ReadRequestOptions = {},
  ): Promise<GitDiffView> {
    const params = new URLSearchParams({ path: file.path, group: file.group });
    if (file.orig_path) params.set("orig_path", file.orig_path);
    return (
      await request<{ diff: GitDiffView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/diff?${params}`,
        options,
      )
    ).diff;
  }

  async function getWorkspaceGitBranches(
    workspaceId: string,
    options: ReadRequestOptions = {},
  ): Promise<GitBranchesView> {
    return (
      await request<{ branches: GitBranchesView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/branches`,
        options,
      )
    ).branches;
  }

  async function getWorkspaceGitCommits(
    workspaceId: string,
    ref: string,
    skip: number,
    options: ReadRequestOptions = {},
  ): Promise<GitCommitsPage> {
    const params = new URLSearchParams({ ref, skip: String(skip) });
    return (
      await request<{ commits: GitCommitsPage }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/commits?${params}`,
        options,
      )
    ).commits;
  }

  async function getWorkspaceGitCommit(
    workspaceId: string,
    sha: string,
    options: ReadRequestOptions = {},
  ): Promise<GitCommitView> {
    return (
      await request<{ commit: GitCommitView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/commits/${encodeURIComponent(sha)}`,
        options,
      )
    ).commit;
  }

  async function getWorkspaceGitCommitDiff(
    workspaceId: string,
    sha: string,
    file: { path: string; orig_path?: string },
    options: ReadRequestOptions = {},
  ): Promise<GitCommitDiffView> {
    const params = new URLSearchParams({ path: file.path });
    if (file.orig_path) params.set("orig_path", file.orig_path);
    return (
      await request<{ diff: GitCommitDiffView }>(
        `/workspaces/${encodeURIComponent(workspaceId)}/git/commits/${encodeURIComponent(sha)}/diff?${params}`,
        options,
      )
    ).diff;
  }

  async function listWorkspaceRoots(
    options: ReadRequestOptions = {},
  ): Promise<WorkspaceRootView[]> {
    return (await request<{ roots: WorkspaceRootView[] }>("/workspace-roots", options)).roots;
  }

  async function listWorkspaceRootEntries(
    rootId: string,
    path = "",
    options: ReadRequestOptions = {},
  ): Promise<WorkspaceDirectoryListingView> {
    const query = path ? `?path=${encodeURIComponent(path)}` : "";
    return request<WorkspaceDirectoryListingView>(
      `/workspace-roots/${encodeURIComponent(rootId)}/entries${query}`,
      options,
    );
  }

  async function listWorkspaceFilePickerEntries(
    workspaceId: string,
    query = "",
    options: ReadRequestOptions & { limit?: number } = {},
  ): Promise<FilePickerResultView> {
    const params = new URLSearchParams();
    if (query) params.set("query", query);
    if (options.limit !== undefined) params.set("limit", String(options.limit));
    const path = `/workspaces/${encodeURIComponent(workspaceId)}/file-picker${params.toString() ? `?${params.toString()}` : ""}`;
    return request<FilePickerResultView>(path, { signal: options.signal });
  }

  async function createSession(input: CreateSessionInput): Promise<CreateSessionResult> {
    return request<CreateSessionResult>("/sessions", {
      method: "POST",
      body: input,
      mutating: true,
    });
  }

  async function updateSession(sessionId: string, input: UpdateSessionInput): Promise<SessionView> {
    return (
      await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}`, {
        method: "PATCH",
        body: input,
        mutating: true,
      })
    ).session;
  }

  async function pinSession(sessionId: string): Promise<SessionView> {
    return (
      await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}/pin`, {
        method: "POST",
        mutating: true,
      })
    ).session;
  }

  async function unpinSession(sessionId: string): Promise<SessionView> {
    return (
      await request<{ session: SessionView }>(`/sessions/${encodeURIComponent(sessionId)}/unpin`, {
        method: "POST",
        mutating: true,
      })
    ).session;
  }

  async function archiveSession(sessionId: string): Promise<SessionView> {
    return (
      await request<{ session: SessionView }>(
        `/sessions/${encodeURIComponent(sessionId)}/archive`,
        {
          method: "POST",
          mutating: true,
        },
      )
    ).session;
  }

  async function unarchiveSession(sessionId: string): Promise<SessionView> {
    return (
      await request<{ session: SessionView }>(
        `/sessions/${encodeURIComponent(sessionId)}/unarchive`,
        {
          method: "POST",
          mutating: true,
        },
      )
    ).session;
  }

  async function getSession(
    sessionId: string,
    options: ReadRequestOptions = {},
  ): Promise<SessionView> {
    return (
      await boundedReadRequest<{ session: SessionView }>(
        `/sessions/${encodeURIComponent(sessionId)}`,
        options,
      )
    ).session;
  }

  // GET /sessions/:id/turns is read-only turn history. WebUI dispatch must use
  // submitInboxMessage(); reported Agent facts remain authoritative for turn lifecycle.
  async function listTurns(
    sessionId: string,
    options: ReadRequestOptions = {},
  ): Promise<TurnView[]> {
    return (
      await boundedReadRequest<{ turns: TurnView[] }>(
        `/sessions/${encodeURIComponent(sessionId)}/turns`,
        options,
      )
    ).turns;
  }

  async function listInboxMessages(
    sessionId: string,
    options: ReadRequestOptions = {},
  ): Promise<InboxMessageView[]> {
    return (
      await boundedReadRequest<{ inbox_messages: InboxMessageView[] }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages`,
        options,
      )
    ).inbox_messages;
  }

  async function forkSession(
    sessionId: string,
    turnId: string,
    operationId: string,
  ): Promise<ForkSessionOutcome> {
    return request(`/sessions/${encodeURIComponent(sessionId)}/fork`, {
      method: "POST",
      body: { turn_id: turnId, operation_id: operationId },
      mutating: true,
      idempotency: false,
    });
  }

  async function submitInboxMessage(
    sessionId: string,
    input: SubmitInboxMessageInput,
    messageId: string,
  ): Promise<InboxMessageView> {
    return (
      await request<{ inbox_message: InboxMessageView }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}`,
        { method: "PUT", body: input },
      )
    ).inbox_message;
  }

  async function getInboxMessage(
    sessionId: string,
    messageId: string,
    options: ReadRequestOptions = {},
  ): Promise<InboxMessageView> {
    return (
      await boundedReadRequest<{ inbox_message: InboxMessageView }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}`,
        options,
      )
    ).inbox_message;
  }

  async function retryInboxMessage(
    sessionId: string,
    originalId: string,
    messageId: string,
    allowUnknown: boolean,
  ): Promise<InboxMessageView> {
    return (
      await request<{ inbox_message: InboxMessageView }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(originalId)}/retry`,
        { method: "POST", body: { message_id: messageId, allow_unknown: allowUnknown } },
      )
    ).inbox_message;
  }

  async function cancelInboxMessage(
    sessionId: string,
    messageId: string,
  ): Promise<InboxMessageView> {
    return (
      await request<{ inbox_message: InboxMessageView }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}/cancel`,
        { method: "POST", mutating: true },
      )
    ).inbox_message;
  }

  async function dismissInboxMessage(
    sessionId: string,
    messageId: string,
  ): Promise<InboxMessageView> {
    return (
      await request<{ inbox_message: InboxMessageView }>(
        `/sessions/${encodeURIComponent(sessionId)}/inbox/messages/${encodeURIComponent(messageId)}/dismiss`,
        { method: "POST", mutating: true },
      )
    ).inbox_message;
  }

  async function listEvents(
    sessionId: string,
    options: ReadRequestOptions = {},
  ): Promise<EventView[]> {
    return (
      await boundedReadRequest<{ events: EventView[] }>(
        `/sessions/${encodeURIComponent(sessionId)}/events`,
        options,
      )
    ).events;
  }

  async function getTurnTimeline(
    sessionId: string,
    options: {
      direction: TurnTimelineDirection;
      historySource?: import("./types").HistorySource | null;
      turnId?: string | null;
      limit?: number;
      signal?: AbortSignal;
    },
  ): Promise<TurnTimelinePage> {
    const params = new URLSearchParams({ direction: options.direction });
    if (options.historySource) params.set("history_source", options.historySource);
    if (options.turnId) params.set("turn_id", options.turnId);
    if (options.limit !== undefined) params.set("limit", String(options.limit));
    return boundedReadRequest<TurnTimelinePage>(
      `/sessions/${encodeURIComponent(sessionId)}/turns/timeline?${params.toString()}`,
      { signal: options.signal },
    );
  }

  async function getTurnTreeHistory(
    sessionId: string,
    options: { fromTurnId?: string | null; limit?: number; signal?: AbortSignal } = {},
  ): Promise<TurnTreeHistoryPage> {
    const params = new URLSearchParams();
    if (options.fromTurnId) params.set("from_turn_id", options.fromTurnId);
    if (options.limit !== undefined) params.set("limit", String(options.limit));
    const query = params.size ? `?${params.toString()}` : "";
    return boundedReadRequest<TurnTreeHistoryPage>(
      `/sessions/${encodeURIComponent(sessionId)}/turns/tree/history${query}`,
      { signal: options.signal },
    );
  }

  async function getTurnTreeUpdates(
    sessionId: string,
    options: { fromTurnId?: string | null; signal?: AbortSignal } = {},
  ): Promise<TurnTreeUpdatesPage> {
    const params = new URLSearchParams();
    if (options.fromTurnId) params.set("from_turn_id", options.fromTurnId);
    const query = params.size ? `?${params.toString()}` : "";
    return boundedReadRequest<TurnTreeUpdatesPage>(
      `/sessions/${encodeURIComponent(sessionId)}/turns/tree/updates${query}`,
      { signal: options.signal },
    );
  }

  async function interruptSession(sessionId: string): Promise<unknown> {
    return request(`/sessions/${sessionId}/interrupt`, { method: "POST", mutating: true });
  }

  function compactSession(sessionId: string): Promise<SessionCompactionAccepted> {
    return request(`/sessions/${encodeURIComponent(sessionId)}/compact`, {
      method: "POST",
      mutating: true,
    });
  }

  async function resumeSession(sessionId: string): Promise<unknown> {
    return request(`/sessions/${sessionId}/resume`, { method: "POST", mutating: true });
  }

  async function terminateSession(sessionId: string): Promise<unknown> {
    return request(`/sessions/${encodeURIComponent(sessionId)}/exit`, {
      method: "POST",
      mutating: true,
    });
  }

  return {
    listBots,
    listBotClients,
    listAgentClientModels,
    getBotMessages,
    getBotMessageContext,
    searchBotMessages,
    getBotTodos,
    changeBotTodo,
    saveBot,
    getBotDeletionBlockers,
    deleteBot,
    sendBotMessage,
    retryBotMessage,
    listAgentClients,
    patchAgentClient,
    resetAgentClient,
    setupAgentClient,
    getAgentClientSetup,
    listSessionCreationOptions,
    listPendingInteractions,
    getInteraction,
    respondInteraction,
    validateExternalApiToken,
    listAgentProfiles,
    getUserFile,
    replaceUserFile,
    listMemories,
    createAgentProfile,
    deleteAgentProfile,
    getAgentProfile,
    updateAgentProfile,
    listWorkflows,
    getWorkflow,
    getWorkflowRevision,
    listWorkflowPatches,
    getWorkflowDocument,
    pauseWorkflow,
    resumeWorkflow,
    retryWorkflow,
    getSessionOverview,
    listWorkspaces,
    registerWorkspace,
    renameWorkspace,
    deleteWorkspace,
    refreshWorkspaceGitStatus,
    getWorkspaceGitChanges,
    getWorkspaceGitDiff,
    getWorkspaceGitBranches,
    getWorkspaceGitCommits,
    getWorkspaceGitCommit,
    getWorkspaceGitCommitDiff,
    listWorkspaceRoots,
    listWorkspaceRootEntries,
    listWorkspaceFilePickerEntries,
    createSession,
    updateSession,
    pinSession,
    unpinSession,
    archiveSession,
    unarchiveSession,
    getSession,
    listTurns,
    listInboxMessages,
    submitInboxMessage,
    forkSession,
    getInboxMessage,
    retryInboxMessage,
    cancelInboxMessage,
    dismissInboxMessage,
    listEvents,
    getTurnTimeline,
    getTurnTreeHistory,
    getTurnTreeUpdates,
    interruptSession,
    compactSession,
    resumeSession,
    terminateSession,
  };
}
