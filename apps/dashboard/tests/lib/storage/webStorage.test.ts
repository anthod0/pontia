import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  PersistedState,
  readStored,
  readStoredWith,
  scopedKey,
  textSerializer,
  writeStored,
} from "../../../src/lib/storage";

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

function storageUnavailable(area: "localStorage" | "sessionStorage" = "localStorage") {
  vi.spyOn(window, area, "get").mockImplementation(() => {
    throw new DOMException("The operation is insecure.", "SecurityError");
  });
}

function storageFull() {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Quota exceeded", "QuotaExceededError");
  });
}

test("scoped keys append one scope as is and several as JSON", () => {
  expect(scopedKey("pontia.lastPage", "office-mac")).toBe("pontia.lastPage:office-mac");
  expect(scopedKey("pontia:bot-quote:v1", "local", "bot:1")).toBe(
    'pontia:bot-quote:v1:["local","bot:1"]',
  );
});

test("reads and writes local or session storage and removes the key for null", () => {
  expect(writeStored("pontia.a", "1")).toBe(true);
  writeStored("pontia.b", "2", { area: "session" });
  expect(readStored("pontia.a")).toBe("1");
  expect(readStored("pontia.b")).toBeNull();
  expect(readStored("pontia.b", { area: "session" })).toBe("2");

  writeStored("pontia.a", null);
  expect(localStorage.getItem("pontia.a")).toBeNull();
  expect(localStorage.length).toBe(0);
});

test("unreadable data falls back to null", () => {
  localStorage.setItem("pontia.json", "{broken");
  localStorage.setItem("pontia.width", "-1");
  expect(readStoredWith("pontia.json", (saved) => JSON.parse(saved))).toBeNull();
  const width = (saved: string) => (Number(saved) > 0 ? Number(saved) : undefined);
  expect(readStoredWith("pontia.width", width)).toBeNull();
  expect(readStoredWith("pontia.missing", width)).toBeNull();
});

test("unavailable storage reads as empty and ignores writes unless they are required", () => {
  storageUnavailable();
  expect(readStored("pontia.a")).toBeNull();
  expect(writeStored("pontia.a", "1")).toBe(false);
  expect(writeStored("pontia.a", null)).toBe(false);
  expect(() => writeStored("pontia.a", "1", { required: true })).toThrow(DOMException);
});

test("full storage ignores writes unless they are required", () => {
  storageFull();
  expect(writeStored("pontia.a", "1")).toBe(false);
  expect(() => writeStored("pontia.a", "1", { required: true })).toThrow("Quota exceeded");
});

const choice = {
  parse: (saved: string) => (saved === "a" || saved === "b" ? saved : undefined),
  stringify: (value: "default" | "a" | "b") => (value === "default" ? null : value),
};

test("persisted state reads the saved value and saves replacements", () => {
  localStorage.setItem("pontia.choice", "a");
  const state = new PersistedState<"default" | "a" | "b">("pontia.choice", "default", choice);
  expect(state.current).toBe("a");
  state.current = "b";
  expect(localStorage.getItem("pontia.choice")).toBe("b");
  expect(new PersistedState("pontia.choice", "default", choice).current).toBe("b");
});

test("persisted state does not write its default until it changes", () => {
  new PersistedState<"default" | "a" | "b">("pontia.choice", "default", choice);
  expect(localStorage.length).toBe(0);
});

test("the default value removes the key instead of saving it", () => {
  localStorage.setItem("pontia.choice", "a");
  const state = new PersistedState<"default" | "a" | "b">("pontia.choice", "default", choice);
  state.current = "default";
  expect(state.current).toBe("default");
  expect(localStorage.getItem("pontia.choice")).toBeNull();

  const text = new PersistedState("pontia.text", "saved", textSerializer);
  text.current = null;
  expect(localStorage.getItem("pontia.text")).toBeNull();
  text.current = "";
  expect(localStorage.getItem("pontia.text")).toBe("");
});

test("bad saved data falls back to the default", () => {
  localStorage.setItem("pontia.choice", "unknown");
  localStorage.setItem("pontia.json", "{broken");
  expect(new PersistedState("pontia.choice", "default", choice).current).toBe("default");
  const json = {
    parse: (saved: string) => JSON.parse(saved) as number[],
    stringify: JSON.stringify,
  };
  expect(new PersistedState("pontia.json", [], json).current).toEqual([]);
});

test("without storage the state keeps working in memory", () => {
  storageUnavailable();
  const state = new PersistedState<"default" | "a" | "b">("pontia.choice", "default", choice);
  expect(state.current).toBe("default");
  state.current = "a";
  expect(state.current).toBe("a");
});

test("full storage keeps the new value in memory", () => {
  const state = new PersistedState<"default" | "a" | "b">("pontia.choice", "default", choice);
  storageFull();
  state.current = "a";
  expect(state.current).toBe("a");
  expect(localStorage.getItem("pontia.choice")).toBeNull();
});

test("persisted state does not follow writes from another tab", () => {
  const state = new PersistedState("pontia.text", null, textSerializer);
  localStorage.setItem("pontia.text", "other tab");
  window.dispatchEvent(new StorageEvent("storage", { key: "pontia.text", newValue: "other tab" }));
  expect(state.current).toBeNull();
});

test("states for different scopes are independent", () => {
  const office = new PersistedState(scopedKey("pontia.x", "office"), null, textSerializer);
  const laptop = new PersistedState(scopedKey("pontia.x", "laptop"), null, textSerializer);
  office.current = "1";
  expect(laptop.current).toBeNull();
  expect(localStorage.getItem("pontia.x:office")).toBe("1");
  expect(localStorage.getItem("pontia.x:laptop")).toBeNull();
});
