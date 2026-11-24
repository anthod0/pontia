import type { MessageKey } from "$lib/i18n";
import type { Component } from "svelte";
import BrainIcon from "phosphor-svelte/lib/BrainIcon";
import FolderIcon from "phosphor-svelte/lib/FolderIcon";
import RobotIcon from "phosphor-svelte/lib/RobotIcon";
import SlidersHorizontalIcon from "phosphor-svelte/lib/SlidersHorizontalIcon";
import TerminalWindowIcon from "phosphor-svelte/lib/TerminalWindowIcon";

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
    icon: TerminalWindowIcon,
    match: ["/settings/clients"],
  },
  {
    label: "profiles.title",
    path: "/settings/agent-profiles",
    icon: RobotIcon,
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
    icon: BrainIcon,
    match: ["/settings/memory"],
  },
];

export function isSettingsPath(path: string): boolean {
  return path === "/settings" || path.startsWith("/settings/");
}

export function isSettingsSectionActive(section: SettingsSection, path: string): boolean {
  return section.match.includes(path);
}
