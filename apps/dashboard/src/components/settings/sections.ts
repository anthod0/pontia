import type { MessageKey } from "$lib/i18n";
import type { Component } from "svelte";
import BooksIcon from "phosphor-svelte/lib/BooksIcon";
import FolderIcon from "phosphor-svelte/lib/FolderIcon";
import ScrollIcon from "phosphor-svelte/lib/ScrollIcon";
import SlidersHorizontalIcon from "phosphor-svelte/lib/SlidersHorizontalIcon";
import TerminalIcon from "phosphor-svelte/lib/TerminalIcon";

export type SettingsSection = {
  label: MessageKey;
  path: string;
  match: string[];
  icon: Component;
};

export const settingsSections: SettingsSection[] = [
  {
    label: "settings.common",
    path: "/settings/common",
    icon: SlidersHorizontalIcon,
    match: ["/settings/common"],
  },
  {
    label: "settings.agentClients",
    path: "/settings/clients",
    icon: TerminalIcon,
    match: ["/settings/clients"],
  },
  {
    label: "profiles.title",
    path: "/settings/agent-profiles",
    icon: ScrollIcon,
    match: ["/settings/agent-profiles"],
  },
  {
    label: "settings.workspaces",
    path: "/settings/workspaces",
    icon: FolderIcon,
    match: ["/settings/workspaces", "/workspaces"],
  },
  {
    label: "settings.memory",
    path: "/settings/memory",
    icon: BooksIcon,
    match: ["/settings/memory"],
  },
];

export function isSettingsPath(path: string): boolean {
  return path === "/settings" || path.startsWith("/settings/");
}

export function isSettingsSectionActive(section: SettingsSection, path: string): boolean {
  return section.match.includes(path);
}
