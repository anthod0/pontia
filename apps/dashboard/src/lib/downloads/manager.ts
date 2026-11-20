import { writable } from "svelte/store";
import { translate } from "$lib/i18n";
import { ApiError } from "../../api/errors";
import type { DeviceRuntime } from "../../device/runtime";
import { saveDownload, type DownloadProgress } from "./save";

export type DownloadFile = { kind: "shared"; fileId: string } | { kind: "path"; path: string };
export type DownloadTask = {
  id: string;
  key: string;
  deviceId: string;
  deviceName: string | null;
  sessionId: string;
  file: DownloadFile;
  name: string;
  status: "receiving" | "complete" | "failed" | "cancelled";
  progress: DownloadProgress;
  error: string | null;
  /** API error code of the last failure, such as `not_found` when the file is gone. */
  errorCode: string | null;
};

export function downloadKey(deviceId: string, sessionId: string, file: DownloadFile): string {
  return JSON.stringify([
    deviceId,
    sessionId,
    file.kind,
    file.kind === "shared" ? file.fileId : file.path,
  ]);
}

export function createDownloads() {
  let tasks: DownloadTask[] = [];
  const store = writable(tasks);
  const devices = new Map<string, Pick<DeviceRuntime, "id" | "name" | "fileApi">>();
  const running = new Map<string, AbortController>();
  const cancelled = new DOMException("Download cancelled", "AbortError");
  const emit = () => store.set(tasks.map((task) => ({ ...task })));

  async function receive(task: DownloadTask) {
    const controller = new AbortController();
    running.set(task.id, controller);
    task.status = "receiving";
    task.progress = { received: 0, total: null };
    task.error = null;
    task.errorCode = null;
    emit();
    try {
      const device = devices.get(task.deviceId);
      if (!device) throw new Error(translate("downloads.deviceUnavailable"));
      const response = await (task.file.kind === "shared"
        ? device.fileApi.sharedFileContent(task.sessionId, task.file.fileId, controller.signal)
        : device.fileApi.pathFileContent(task.sessionId, task.file.path, controller.signal));
      await saveDownload(
        response,
        task.name,
        (progress) => {
          task.progress = progress;
          emit();
        },
        controller.signal,
      );
      task.status = "complete";
    } catch (error) {
      if (controller.signal.reason !== cancelled) {
        task.status = "failed";
        task.errorCode = error instanceof ApiError ? error.code : null;
        task.error =
          error instanceof ApiError && error.status === 404
            ? translate("files.missing")
            : error instanceof Error
              ? error.message
              : String(error);
      }
    } finally {
      running.delete(task.id);
      emit();
    }
  }

  return {
    subscribe: store.subscribe,
    registerDevice(device: Pick<DeviceRuntime, "id" | "name" | "fileApi">) {
      devices.set(device.id, device);
      return () => {
        if (devices.get(device.id) !== device) return;
        devices.delete(device.id);
        for (const task of tasks) {
          if (task.deviceId === device.id)
            running.get(task.id)?.abort(new Error(translate("downloads.deviceUnavailable")));
        }
      };
    },
    start(deviceId: string, sessionId: string, file: DownloadFile, name: string) {
      const key = downloadKey(deviceId, sessionId, file);
      const existing = tasks.find(
        (task) => task.key === key && (task.status === "receiving" || task.status === "failed"),
      );
      if (existing) {
        if (existing.status === "failed") void receive(existing);
        return existing;
      }
      const task: DownloadTask = {
        id: crypto.randomUUID(),
        key,
        deviceId,
        sessionId,
        file,
        name,
        deviceName: devices.get(deviceId)?.name ?? null,
        status: "receiving",
        progress: { received: 0, total: null },
        error: null,
        errorCode: null,
      };
      tasks.unshift(task);
      void receive(task);
      return task;
    },
    cancel(id: string) {
      const task = tasks.find((task) => task.id === id);
      if (!task || task.status !== "receiving") return;
      task.status = "cancelled";
      running.get(task.id)?.abort(cancelled);
      emit();
    },
    retry(id: string) {
      const task = tasks.find((task) => task.id === id);
      if (task?.status === "failed") void receive(task);
    },
    clearFinished() {
      tasks = tasks.filter((task) => task.status === "receiving");
      emit();
    },
  };
}

export const downloads = createDownloads();
