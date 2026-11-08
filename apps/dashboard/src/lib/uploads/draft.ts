const CHUNK_SIZE = 1 << 20;
import { writable } from "svelte/store";

import { sha256 } from "@noble/hashes/sha2.js";

import { ApiError } from "../../api/errors";
import { type Upload } from "../../api/uploads";

import {
  loadUploadRecords,
  saveUploadRecord,
  removeUploadRecord,
  type UploadRecord,
} from "./records";
import type { DeviceRuntime } from "../../device/runtime";

export type UploadItem = {
  id: string;
  name: string;
  size: number;
  status: "hashing" | "uploading" | "select" | "failed" | "complete";
  offset: number;
  record?: UploadRecord;
  upload?: Upload;
  file?: File;
  error?: string;
  saved: boolean;
  recreate?: boolean;
};

export type UploadDraftState = { ready: boolean; items: UploadItem[]; warning: string | null };

export async function fileDigest(file: Blob, signal?: AbortSignal): Promise<string> {
  const hash = sha256.create();
  for (let offset = 0; offset < file.size; offset += CHUNK_SIZE) {
    signal?.throwIfAborted();
    hash.update(new Uint8Array(await file.slice(offset, offset + CHUNK_SIZE).arrayBuffer()));
  }
  return Array.from(hash.digest(), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
export function createUploadDrafts(device: DeviceRuntime) {
  const retainedDrafts = new Map<string, ReturnType<typeof createUploadDraft>>();
  function uploadKey(): string {
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }

  function createUploadDraft(draft: string) {
    let state: UploadDraftState = { ready: false, items: [], warning: null };
    const store = writable(state);
    const lifetime = new AbortController();
    const running = new Map<string, AbortController>();
    let scope: string;
    function emit() {
      store.set({ ...state, items: [...state.items] });
    }
    function warn(error: unknown) {
      state.warning = `Cannot save upload recovery. Resuming after closing this page is not guaranteed. ${error instanceof Error ? error.message : String(error)}`;
      emit();
    }
    async function save(item: UploadItem) {
      try {
        await saveUploadRecord(item.record!);
        item.saved = true;
      } catch (error) {
        item.saved = false;
        warn(error);
      }
    }
    async function forget(item: UploadItem) {
      if (!item.record) return;
      await removeUploadRecord(item.record);
    }
    function active(item: UploadItem, signal: AbortSignal) {
      signal.throwIfAborted();
      if (!state.items.includes(item)) throw new DOMException("Attachment removed", "AbortError");
    }
    async function transfer(item: UploadItem, file?: File) {
      if (running.has(item.id)) return;
      const controller = new AbortController();
      running.set(item.id, controller);
      const signal = AbortSignal.any([controller.signal, lifetime.signal]);
      try {
        item.error = undefined;
        if (file) {
          item.status = "hashing";
          emit();
          if (file.size > 1 << 30) throw new Error("Files must be 1 GiB or smaller");
          const digest = await fileDigest(file, signal);
          active(item, signal);
          if (item.record && (item.record.size !== file.size || item.record.sha256 !== digest)) {
            throw new Error("Select the original file: size and SHA-256 must match");
          }
          item.file = file;
          if (!item.record) {
            item.record = {
              scope,
              draft,
              key: item.id,
              uploadId: null,
              name: file.name,
              size: file.size,
              sha256: digest,
            };
            await save(item);
          }
        }
        active(item, signal);
        if (!item.record) throw new Error("Select the file again");
        if (item.recreate)
          throw new Error("Checksum mismatch. Remove this attachment and upload the file again");
        item.status = "uploading";
        emit();
        const record = item.record;
        let upload: Upload;
        if (record.uploadId)
          upload = await device.uploads.uploadRequest(`/${record.uploadId}`, { signal });
        else {
          const query = new URLSearchParams({
            name: record.name,
            size: String(record.size),
            sha256: record.sha256,
          });
          upload = await device.uploads.uploadRequest(`?${query}`, {
            method: "POST",
            headers: { "Idempotency-Key": record.key },
            signal,
          });
          active(item, signal);
          record.uploadId = upload.upload_id;
          await save(item);
        }
        active(item, signal);
        item.upload = upload;
        item.offset = upload.offset;
        emit();
        if (!upload.completed && !item.file && upload.offset < upload.size) {
          item.status = "select";
          return;
        }
        while (!upload.completed && upload.offset < upload.size) {
          active(item, signal);
          const body = await item
            .file!.slice(upload.offset, upload.offset + CHUNK_SIZE)
            .arrayBuffer();
          active(item, signal);
          upload = await device.uploads.uploadRequest(
            `/${upload.upload_id}?offset=${upload.offset}`,
            {
              method: "PATCH",
              headers: { "Content-Type": "application/octet-stream" },
              body,
              signal,
            },
          );
          active(item, signal);
          item.offset = upload.offset;
          emit();
        }
        if (!upload.completed)
          upload = await device.uploads.uploadRequest(`/${upload.upload_id}/complete`, {
            method: "POST",
            signal,
          });
        active(item, signal);
        item.upload = upload;
        item.status = "complete";
        item.file = undefined;
      } catch (error) {
        if (!signal.aborted) {
          item.status = "failed";
          item.error = error instanceof Error ? error.message : String(error);
          item.recreate = error instanceof ApiError && error.code === "upload_checksum_mismatch";
        }
      } finally {
        running.delete(item.id);
        emit();
      }
    }
    const initialized = (async () => {
      try {
        scope = await device.access.uploadStorageScope();
        lifetime.signal.throwIfAborted();
        let records: UploadRecord[] = [];
        try {
          records = await loadUploadRecords(scope, draft);
        } catch (error) {
          warn(error);
        }
        lifetime.signal.throwIfAborted();
        state.items = records.map((record) => ({
          id: record.key,
          name: record.name,
          size: record.size,
          status: "uploading",
          offset: 0,
          record,
          saved: true,
        }));
        state.ready = true;
        emit();
        await Promise.all(state.items.map((item) => transfer(item)));
      } catch (error) {
        if (!lifetime.signal.aborted) {
          state.warning = error instanceof Error ? error.message : String(error);
          emit();
        }
      }
    })();
    return {
      subscribe: store.subscribe,
      initialized,
      async add(files: File[]) {
        await initialized;
        if (!state.ready || lifetime.signal.aborted) return;
        for (const file of files) {
          const item: UploadItem = {
            id: uploadKey(),
            name: file.name,
            size: file.size,
            status: "hashing",
            offset: 0,
            saved: false,
          };
          state.items.push(item);
          emit();
          void transfer(item, file);
        }
      },
      retry(item: UploadItem, file?: File) {
        return transfer(item, file);
      },
      reconnect() {
        for (const item of state.items)
          if (item.status === "failed" && !item.recreate) void transfer(item);
      },
      async remove(item: UploadItem) {
        running.get(item.id)?.abort();
        try {
          await forget(item);
        } catch (error) {
          state.warning = `Could not remove this attachment's recovery record. Retry removal before leaving this page. ${String(error)}`;
          emit();
          return;
        }
        state.items = state.items.filter((value) => value !== item);
        emit();
      },
      async clear() {
        const items = [...state.items];
        for (const item of items) await forget(item);
        state.items = state.items.filter((item) => !items.includes(item));
        emit();
      },
      destroy() {
        lifetime.abort();
      },
    };
  }
  return {
    createUploadDraft,
    getRetainedUploadDraft(draft: string) {
      let retained = retainedDrafts.get(draft);
      if (!retained) {
        retained = createUploadDraft(draft);
        retainedDrafts.set(draft, retained);
      }
      return retained;
    },
    dispose() {
      for (const draft of retainedDrafts.values()) draft.destroy();
      retainedDrafts.clear();
    },
  };
}

export type UploadDraft = ReturnType<ReturnType<typeof createUploadDrafts>["createUploadDraft"]>;
