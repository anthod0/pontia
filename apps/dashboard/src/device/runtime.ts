import { QueryClient } from "@tanstack/svelte-query";
import type { ApiAccess } from "./apiAccess";
import { createApi } from "../api/client";
import { createUploads } from "../api/uploads";
import { createFileApi } from "../api/sessionFiles";
import { createConnection } from "../stores/connection";
import { createChatDraft } from "../stores/chatDraft";
import { createInboxRecovery } from "../stores/inboxRecovery";
import { createSessions } from "../stores/sessions";
import { createWorkflows } from "../stores/workflows";
import { createTimeline } from "../stores/timeline";
import { createSessionFiles } from "../stores/sessionFiles";
import { createSessionQueries } from "../queries/sessions";
import { createWorkspaceQueries } from "../queries/workspaces";
import { createWorkflowQueries } from "../queries/workflows";
import { createInboxQueries } from "../queries/inbox";
import { createOptimisticChat } from "../queries/optimisticChat";
import { createAgentProfileQueries } from "../queries/agentProfiles";
import { createCompactionHistory } from "../queries/compactionHistory";
import { createInvalidation } from "../queries/dashboardInvalidation";
import { createSessionCreation } from "../queries/sessionCreation";
import { createSessionOverview } from "../queries/sessionOverview";
import { createTimelineQueries } from "../queries/timeline";
import { createLifecycle } from "../services/dashboardRuntime";
import { createSnapshotRefresh } from "../services/dashboardSnapshotRefresh";
import { createEvents } from "../services/eventStream";
import { createLiveOutput } from "../services/liveOutputStream";
import { createUploadDrafts } from "../lib/uploads/draft";
import { createPreviews } from "../lib/downloads/preview";

export class DeviceRuntime {
  readonly queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  constructor(
    readonly id: string,
    readonly access: ApiAccess,
  ) {
    this.queryClient.mount();
  }

  private _api?: ReturnType<typeof createApi>;
  get api(): ReturnType<typeof createApi> {
    return (this._api ??= createApi(this));
  }

  private _uploads?: ReturnType<typeof createUploads>;
  get uploads(): ReturnType<typeof createUploads> {
    return (this._uploads ??= createUploads(this));
  }

  private _fileApi?: ReturnType<typeof createFileApi>;
  get fileApi(): ReturnType<typeof createFileApi> {
    return (this._fileApi ??= createFileApi(this));
  }

  private _connection?: ReturnType<typeof createConnection>;
  get connection(): ReturnType<typeof createConnection> {
    return (this._connection ??= createConnection(this));
  }

  private _chatDraft?: ReturnType<typeof createChatDraft>;
  get chatDraft(): ReturnType<typeof createChatDraft> {
    return (this._chatDraft ??= createChatDraft(this));
  }

  private _inboxRecovery?: ReturnType<typeof createInboxRecovery>;
  get inboxRecovery(): ReturnType<typeof createInboxRecovery> {
    return (this._inboxRecovery ??= createInboxRecovery(this));
  }

  private _sessions?: ReturnType<typeof createSessions>;
  get sessions(): ReturnType<typeof createSessions> {
    return (this._sessions ??= createSessions(this));
  }

  private _workflows?: ReturnType<typeof createWorkflows>;
  get workflows(): ReturnType<typeof createWorkflows> {
    return (this._workflows ??= createWorkflows(this));
  }

  private _timeline?: ReturnType<typeof createTimeline>;
  get timeline(): ReturnType<typeof createTimeline> {
    return (this._timeline ??= createTimeline(this));
  }

  private _sessionFiles?: ReturnType<typeof createSessionFiles>;
  get sessionFiles(): ReturnType<typeof createSessionFiles> {
    return (this._sessionFiles ??= createSessionFiles(this));
  }

  private _sessionQueries?: ReturnType<typeof createSessionQueries>;
  get sessionQueries(): ReturnType<typeof createSessionQueries> {
    return (this._sessionQueries ??= createSessionQueries(this));
  }

  private _workspaceQueries?: ReturnType<typeof createWorkspaceQueries>;
  get workspaceQueries(): ReturnType<typeof createWorkspaceQueries> {
    return (this._workspaceQueries ??= createWorkspaceQueries(this));
  }

  private _workflowQueries?: ReturnType<typeof createWorkflowQueries>;
  get workflowQueries(): ReturnType<typeof createWorkflowQueries> {
    return (this._workflowQueries ??= createWorkflowQueries(this));
  }

  private _inboxQueries?: ReturnType<typeof createInboxQueries>;
  get inboxQueries(): ReturnType<typeof createInboxQueries> {
    return (this._inboxQueries ??= createInboxQueries(this));
  }

  private _optimisticChat?: ReturnType<typeof createOptimisticChat>;
  get optimisticChat(): ReturnType<typeof createOptimisticChat> {
    return (this._optimisticChat ??= createOptimisticChat(this));
  }

  private _agentProfileQueries?: ReturnType<typeof createAgentProfileQueries>;
  get agentProfileQueries(): ReturnType<typeof createAgentProfileQueries> {
    return (this._agentProfileQueries ??= createAgentProfileQueries(this));
  }

  private _compactionHistory?: ReturnType<typeof createCompactionHistory>;
  get compactionHistory(): ReturnType<typeof createCompactionHistory> {
    return (this._compactionHistory ??= createCompactionHistory(this));
  }

  private _invalidation?: ReturnType<typeof createInvalidation>;
  get invalidation(): ReturnType<typeof createInvalidation> {
    return (this._invalidation ??= createInvalidation(this));
  }

  private _sessionCreation?: ReturnType<typeof createSessionCreation>;
  get sessionCreation(): ReturnType<typeof createSessionCreation> {
    return (this._sessionCreation ??= createSessionCreation(this));
  }

  private _sessionOverview?: ReturnType<typeof createSessionOverview>;
  get sessionOverview(): ReturnType<typeof createSessionOverview> {
    return (this._sessionOverview ??= createSessionOverview(this));
  }

  private _timelineQueries?: ReturnType<typeof createTimelineQueries>;
  get timelineQueries(): ReturnType<typeof createTimelineQueries> {
    return (this._timelineQueries ??= createTimelineQueries(this));
  }

  private _lifecycle?: ReturnType<typeof createLifecycle>;
  get lifecycle(): ReturnType<typeof createLifecycle> {
    return (this._lifecycle ??= createLifecycle(this));
  }

  private _snapshotRefresh?: ReturnType<typeof createSnapshotRefresh>;
  get snapshotRefresh(): ReturnType<typeof createSnapshotRefresh> {
    return (this._snapshotRefresh ??= createSnapshotRefresh(this));
  }

  private _events?: ReturnType<typeof createEvents>;
  get events(): ReturnType<typeof createEvents> {
    return (this._events ??= createEvents(this));
  }

  private _liveOutput?: ReturnType<typeof createLiveOutput>;
  get liveOutput(): ReturnType<typeof createLiveOutput> {
    return (this._liveOutput ??= createLiveOutput(this));
  }

  private _uploadDrafts?: ReturnType<typeof createUploadDrafts>;
  get uploadDrafts(): ReturnType<typeof createUploadDrafts> {
    return (this._uploadDrafts ??= createUploadDrafts(this));
  }

  private _previews?: ReturnType<typeof createPreviews>;
  get previews(): ReturnType<typeof createPreviews> {
    return (this._previews ??= createPreviews(this));
  }

  dispose(): void {
    this.queryClient.unmount();
    this.lifecycle.stopDashboardRuntime();
    this.lifecycle.clearDashboardRuntimeState();
    this.access.dispose();
  }
}
