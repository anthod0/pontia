import ChatCircleIcon from "phosphor-svelte/lib/ChatCircleIcon";
import StackIcon from "phosphor-svelte/lib/StackIcon";
import CheckSquareIcon from "phosphor-svelte/lib/CheckSquareIcon";
import DesktopIcon from "phosphor-svelte/lib/DesktopIcon";

export const tabs = [
  { path: "/bots", label: "mobile.chats", icon: ChatCircleIcon },
  { path: "/sessions", label: "sessions.title", icon: StackIcon },
  { path: "/todos", label: "bots.todos", icon: CheckSquareIcon },
  { path: "/settings", label: "devices.device", icon: DesktopIcon },
] as const;
export function parentTab(path: string) {
  if (path.startsWith("/settings/") || path === "/workspaces") return "/settings";
  if (path === "/new" || path.startsWith("/chat/")) return "/sessions";
  return "/bots";
}
