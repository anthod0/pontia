import { writable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

export function createChatDraft(_device: DeviceRuntime) {
  const chatDraft = writable("");

  function setChatDraft(value: string): void {
    chatDraft.set(value);
  }

  function clearChatDraft(): void {
    chatDraft.set("");
  }
  return { chatDraft, setChatDraft, clearChatDraft };
}
