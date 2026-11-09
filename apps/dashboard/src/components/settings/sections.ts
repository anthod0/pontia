import type { Component } from "svelte";
import FolderIcon from "phosphor-svelte/lib/FolderIcon";
import SlidersHorizontalIcon from "phosphor-svelte/lib/SlidersHorizontalIcon";
import TerminalWindowIcon from "phosphor-svelte/lib/TerminalWindowIcon";

export type SettingsSection = {
  label: string;
  path: string;
  match: string[];
  icon: Component;
};

export const settingsSections: SettingsSection[] = [
  {
    label: "Common",
    path: "/settings/common",
    icon: SlidersHorizontalIcon,
    match: ["/settings/common"],
  },
  {
    label: "Agent Clients",
    path: "/settings/clients",
    icon: TerminalWindowIcon,
    match: ["/settings/clients"],
  },
  {
    label: "Workspaces",
    path: "/settings/workspaces",
    icon: FolderIcon,
    match: ["/settings/workspaces", "/workspaces"],
  },
];

export function isSettingsPath(path: string): boolean {
  return path === "/settings" || path.startsWith("/settings/");
}

export function isSettingsSectionActive(section: SettingsSection, path: string): boolean {
  return section.match.includes(path);
}
