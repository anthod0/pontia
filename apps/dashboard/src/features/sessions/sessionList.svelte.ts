import type { DeviceRuntime } from "../../device/runtime";
import { snapshotSessionOverview } from "../../queries/sessionOverview";

export function createSessionList(device: DeviceRuntime) {
  const query = device.sessionOverview.createSessionOverviewQuery();
  const snapshot = $derived(snapshotSessionOverview(query.data));
  const list = $derived([
    ...new Map(
      [...snapshot.pinned, ...snapshot.active, ...snapshot.list].map((session) => [
        session.session_id,
        session,
      ]),
    ).values(),
  ]);
  return {
    query,
    get list() {
      return list;
    },
  };
}
