import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import { clientSessionLabel } from "../../clients";
import { contextUsageSummary } from "$lib/contextUsage";
import type {
  InboxMessageView,
  SessionView,
  WorkspaceGitStatusView,
  WorkspaceView,
} from "../../api/types";

export type SessionMetadataItem = {
  key: string;
  label: string;
  value: string;
  title: string;
};

export function workspaceTitle(workspace: WorkspaceView): string {
  return workspace.name ?? workspace.display_path ?? workspace.workspace_id;
}

export function clientTitle(clientType: string, translate: Translator = defaultTranslate): string {
  return clientType || translate("clients.client");
}

export function sessionContextUsageLabel(
  session: SessionView,
  translate: Translator = defaultTranslate,
): string | null {
  if ((session.capabilities?.context_usage ?? "unsupported") === "unsupported") return null;
  const summary = session.context_usage
    ? contextUsageSummary(
        session.context_usage,
        { includeConfidence: false, includeLabel: false },
        translate,
      )
    : translate("context.waitingShort");
  return summary;
}

export function sessionProfileTitle(session: SessionView): string | null {
  if (!session.execution_profile_id) return null;
  return session.execution_profile_version
    ? `${session.execution_profile_id}@${session.execution_profile_version}`
    : session.execution_profile_id;
}

export function sessionHandleTitle(session: SessionView): string | null {
  const handle = session.handle?.trim();
  return handle || null;
}

export function sessionWorkspace(
  session: SessionView,
  workspaces: WorkspaceView[],
): WorkspaceView | null {
  if (!session.workspace_id) return null;
  return workspaces.find((workspace) => workspace.workspace_id === session.workspace_id) ?? null;
}

export function sessionWorkspaceTitle(
  session: SessionView,
  workspaces: WorkspaceView[],
  translate: Translator = defaultTranslate,
): string {
  const workspace = sessionWorkspace(session, workspaces);
  return (
    workspace?.name ??
    workspace?.display_path ??
    session.workspace ??
    session.workspace_id ??
    translate("workspaces.noWorkspace")
  );
}

export function sessionWorkspacePath(
  session: SessionView,
  workspaces: WorkspaceView[],
  translate: Translator = defaultTranslate,
): string {
  const workspace = sessionWorkspace(session, workspaces);
  return (
    workspace?.canonical_path ??
    workspace?.display_path ??
    session.workspace ??
    session.workspace_id ??
    translate("workspaces.noWorkspace")
  );
}

export function gitStatusLabel(
  status: WorkspaceGitStatusView | undefined,
  translate: Translator = defaultTranslate,
): string {
  if (!status || status.state === "unknown") return translate("git.unknown");
  if (status.state === "error") return translate("git.error");
  return status.clean ? translate("git.clean") : translate("git.dirty");
}

export function gitBranchLabel(
  status: WorkspaceGitStatusView | undefined,
  translate: Translator = defaultTranslate,
): string {
  return status?.branch ?? translate("git.noBranch");
}

export function hasGitChangeCounts(status: WorkspaceGitStatusView | undefined): boolean {
  return (
    !!status &&
    (status.staged_count > 0 ||
      status.unstaged_count > 0 ||
      status.untracked_count > 0 ||
      status.conflicted_count > 0 ||
      status.ahead > 0 ||
      status.behind > 0)
  );
}

export function gitStatusCompactText(
  status: WorkspaceGitStatusView | undefined,
  translate: Translator = defaultTranslate,
): string | null {
  if (!status) return null;
  const parts = [gitBranchLabel(status, translate)];
  if (status.ahead) parts.push(`↑${status.ahead}`);
  if (status.behind) parts.push(`↓${status.behind}`);
  if (hasGitChangeCounts(status)) {
    if (status.staged_count) parts.push(`+${status.staged_count}`);
    if (status.unstaged_count) parts.push(`~${status.unstaged_count}`);
    if (status.untracked_count) parts.push(`?${status.untracked_count}`);
    if (status.conflicted_count) parts.push(`!${status.conflicted_count}`);
  }
  return parts.join(" ");
}

export function gitStatusAriaLabel(
  status: WorkspaceGitStatusView | undefined,
  translate: Translator = defaultTranslate,
): string {
  return translate("git.status", {
    branch: gitBranchLabel(status, translate),
    status: gitStatusLabel(status, translate),
  });
}

export function gitStatusTitle(
  session: SessionView,
  status: WorkspaceGitStatusView | undefined,
  gitStatusErrors: Record<string, string | null | undefined>,
  translate: Translator = defaultTranslate,
): string {
  const error = session.workspace_id ? gitStatusErrors[session.workspace_id] : null;
  return status?.failure ?? error ?? gitStatusAriaLabel(status, translate);
}

export function gitStatusToneClass(status: WorkspaceGitStatusView | undefined): string {
  if (!status || status.state === "unknown") return "text-muted-foreground";
  if (status.state === "error") return "text-destructive";
  return status.clean ? "text-success " : "text-warning ";
}

export function sessionMetadataItems(
  session: SessionView,
  workspaces: WorkspaceView[],
  gitStatus: WorkspaceGitStatusView | undefined,
  gitStatusErrors: Record<string, string | null | undefined>,
  translate: Translator = defaultTranslate,
): SessionMetadataItem[] {
  const items: SessionMetadataItem[] = [
    {
      key: "workspace",
      label: translate("workspace.title"),
      value: sessionWorkspaceTitle(session, workspaces, translate),
      title: sessionWorkspacePath(session, workspaces, translate),
    },
    {
      key: "client",
      label: translate("clients.client"),
      value: clientSessionLabel(session),
      title: clientSessionLabel(session),
    },
  ];
  if (session.model)
    items.push({
      key: "model",
      label: translate("model.title"),
      value: session.model,
      title: session.model,
    });
  if (gitStatus)
    items.push({
      key: "git",
      label: "Git",
      value: `${gitBranchLabel(gitStatus, translate)} · ${gitStatusLabel(gitStatus, translate)}`,
      title: gitStatusTitle(session, gitStatus, gitStatusErrors, translate),
    });
  const contextUsageLabel = sessionContextUsageLabel(session, translate);
  if (contextUsageLabel)
    items.push({
      key: "context",
      label: translate("context.label"),
      value: contextUsageLabel,
      title: translate("context.usage", { usage: contextUsageLabel }),
    });
  const profileTitle = sessionProfileTitle(session);
  if (profileTitle)
    items.push({
      key: "profile",
      label: translate("profiles.profile"),
      value: profileTitle,
      title: profileTitle,
    });
  const handleTitle = sessionHandleTitle(session);
  if (handleTitle)
    items.push({
      key: "handle",
      label: translate("chat.handle"),
      value: handleTitle,
      title: handleTitle,
    });
  return items;
}

export function sessionMetadataSummary(
  items: SessionMetadataItem[],
  translate: Translator = defaultTranslate,
): string {
  if (!items.length) return translate("chat.details");
  return items.map((item) => item.value).join(" · ");
}

export function visibleChatInboxMessages(messages: InboxMessageView[]): InboxMessageView[] {
  return messages.filter(
    (message) => !message.retried_by_message_id && ["pending", "failed"].includes(message.state),
  );
}

export function inboxBadgeVariant(
  message: InboxMessageView,
): "default" | "secondary" | "destructive" | "outline" {
  if (message.state === "failed") return "destructive";
  if (message.state === "dispatching") return "outline";
  return "secondary";
}
