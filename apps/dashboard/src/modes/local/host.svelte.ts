import { fromStore } from "svelte/store";
import { translate } from "$lib/i18n";
import { navigate } from "$lib/navigation";
import type { DashboardHost, HostDevice } from "../../device/hostContext";
import type { DeviceRuntime } from "../../device/runtime";

export function createLocalHost(getRuntime: () => DeviceRuntime): DashboardHost {
  const runtime = $derived(getRuntime());
  const status = $derived(fromStore(runtime.connection.sseStatus));
  const device: HostDevice = {
    get id() {
      return runtime.id;
    },
    get handle() {
      return runtime.id;
    },
    get name() {
      return runtime.name ?? translate("downloads.localDevice");
    },
    get state() {
      if (status.current === "open") return "online";
      if (status.current === "closed" || status.current === "error") return "offline";
      return "connecting";
    },
    get runtime() {
      return runtime;
    },
  };
  function reconnect() {
    runtime.events.stopEventStream();
    runtime.events.startEventStream();
  }
  return {
    mode: "local",
    devices: [device],
    activeDevice: device,
    previousDevice: null,
    canSwitchDevice: false,
    routeScope: undefined,
    statusView: null,
    signInUrl: undefined,
    loadError: null,
    onboarding: false,
    loadingDevices: false,
    checkingDevices: false,
    get connecting() {
      return device.state === "connecting";
    },
    selectDevice() {},
    retryActiveOrRefresh: reconnect,
    retryDevice: reconnect,
    async checkDevices() {
      reconnect();
      return true;
    },
    openNewChat: () => void navigate("/"),
    openSettings: () => void navigate("/settings/common"),
  };
}
