import { beforeEach, expect, test, vi } from "vitest";

const routing = vi.hoisted(() => ({ state: {} as App.PageState }));
const goto = vi.hoisted(() =>
  vi.fn(async (url: string, options: { replaceState?: boolean; state?: App.PageState } = {}) => {
    const method = options.replaceState ? "replaceState" : "pushState";
    window.history[method](options.state ?? {}, "", url);
    routing.state = options.state ?? {};
  }),
);

vi.mock("$app/navigation", () => ({ goto }));
vi.mock("$app/state", () => ({
  page: {
    get state() {
      return routing.state;
    },
    get url() {
      return new URL(window.location.href);
    },
  },
}));
vi.mock("../../src/lib/hooks/shellMode.svelte", () => ({ isMobileShell: () => false }));

import { navigate } from "../../src/lib/navigation";

beforeEach(() => {
  goto.mockClear();
  routing.state = {};
  window.history.replaceState({}, "", "/office-mac/chat/current?panel=git");
});

test("preserves a supported side panel tab through a device entry redirect", async () => {
  await navigate("/", undefined, { scope: "/travel-laptop", preserveSidePanel: true });
  expect(goto).toHaveBeenLastCalledWith("/travel-laptop", {
    state: { deviceSwitchPanel: "git" },
  });

  await navigate("/chat/next", undefined, {
    scope: "/travel-laptop",
    replaceState: true,
    preserveSidePanel: true,
  });
  expect(goto).toHaveBeenLastCalledWith("/travel-laptop/chat/next?panel=git", {
    replaceState: true,
  });
});

test("drops a side panel tab that the restored page does not support", async () => {
  window.history.replaceState({}, "", "/office-mac/bots/pontia?panel=context");
  await navigate("/", undefined, { scope: "/travel-laptop", preserveSidePanel: true });
  await navigate("/chat/next", undefined, {
    scope: "/travel-laptop",
    replaceState: true,
    preserveSidePanel: true,
  });

  expect(goto).toHaveBeenLastCalledWith("/travel-laptop/chat/next", { replaceState: true });
  expect(routing.state.deviceSwitchPanel).toBeUndefined();
});

test("does not carry a side panel onto a page without one", async () => {
  await navigate("/settings/common", undefined, {
    scope: "/travel-laptop",
    preserveSidePanel: true,
  });

  expect(goto).toHaveBeenCalledWith("/travel-laptop/settings/common", {});
});
