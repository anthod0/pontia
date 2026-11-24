import type { HostDevice } from "../../device/hostContext";
export const deviceStateLabel: Record<HostDevice["state"], import("$lib/i18n").MessageKey> = {
  connecting: "devices.connecting",
  online: "devices.online",
  offline: "devices.offline",
  sign_in_required: "auth.required",
};
export const deviceStateDotClass: Record<HostDevice["state"], string> = {
  connecting: "bg-busy",
  online: "bg-idle",
  offline: "bg-inactive",
  sign_in_required: "bg-inactive",
};
