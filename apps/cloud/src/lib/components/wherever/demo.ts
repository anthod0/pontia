import { m } from "$lib/paraglide/messages";
import { codex, claude, pi } from "$lib/bot-stories";
import Laptop from "phosphor-svelte/lib/LaptopIcon";
import HardDrives from "phosphor-svelte/lib/HardDrivesIcon";
import Desktop from "phosphor-svelte/lib/DesktopIcon";
import Coffee from "phosphor-svelte/lib/CoffeeIcon";
import Tree from "phosphor-svelte/lib/TreeIcon";
import Train from "phosphor-svelte/lib/TrainIcon";
import Couch from "phosphor-svelte/lib/CouchIcon";
import Briefcase from "phosphor-svelte/lib/BriefcaseIcon";
import AirplaneTilt from "phosphor-svelte/lib/AirplaneTiltIcon";

export const places = [
  {
    icon: Coffee,
    name: m.wherever_cafe,
    phrase: m.wherever_at_cafe,
    wide: [0.12, 0.62],
    narrow: [0.2, 0.42],
  },
  {
    icon: Tree,
    name: m.wherever_park,
    phrase: m.wherever_at_park,
    wide: [0.3, 0.84],
    narrow: [0.2, 0.64],
  },
  {
    icon: Train,
    name: m.wherever_train,
    phrase: m.wherever_at_train,
    wide: [0.52, 0.88],
    narrow: [0.24, 0.86],
  },
  {
    icon: Couch,
    name: m.wherever_couch,
    phrase: m.wherever_at_couch,
    wide: [0.7, 0.66],
    narrow: [0.8, 0.62],
  },
  {
    icon: Briefcase,
    name: m.wherever_office,
    phrase: m.wherever_at_office,
    wide: [0.89, 0.55],
    narrow: [0.8, 0.4],
    browser: true,
  },
  {
    icon: AirplaneTilt,
    name: m.wherever_gate,
    phrase: m.wherever_at_gate,
    wide: [0.86, 0.87],
    narrow: [0.76, 0.86],
  },
];
export const devices = [
  {
    icon: Laptop,
    name: () => "MacBook Pro",
    short: () => "MacBook",
    location: m.wherever_home_mac,
    agent: codex,
    shape: "egg",
    color: "#d79921",
    tasks: [
      [m.wherever_login, "bun test auth"],
      [m.wherever_settings, "git push origin feat/settings"],
      [m.wherever_copy, "bun run lint --fix"],
    ],
  },
  {
    icon: HardDrives,
    name: m.wherever_linux,
    short: () => "Linux",
    location: m.wherever_lab_linux,
    agent: claude,
    shape: "mochi",
    color: "#689d6a",
    tasks: [
      [m.wherever_cache, "cargo test -p cache"],
      [m.wherever_postgres, "docker compose up -d"],
      [m.wherever_profile, "cargo bench import"],
    ],
  },
  {
    icon: Desktop,
    name: m.wherever_pc,
    short: m.wherever_office,
    location: m.wherever_work_pc,
    agent: pi,
    shape: "pudding",
    color: "#9c6b3c",
    tasks: [
      [m.wherever_api, "gh pr create --draft"],
      [m.wherever_integration, "npm run test:e2e"],
      [m.wherever_sdk, "npm version minor"],
    ],
  },
] as const;
