import { render, screen } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import LocalhostPreviewPanel from "../../src/components/chat/LocalhostPreviewPanel.svelte";
import { parseLocalhostPreviewTarget } from "../../src/lib/localhostPreview";
import { testDevice } from "../deviceFixture";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

test.each([
  ["http://localhost:5173/dashboard", "http://127.0.0.1:5173/"],
  ["http://localhost/dashboard", "http://[::1]/dashboard"],
  ["https://localhost/dashboard", "http://0.0.0.0:443/"],
  ["http://localhost:5173/dashboard", "http://localhost:8091/api"],
])("blocks local self-preview from %s to %s", async (dashboard, href) => {
  vi.stubGlobal("location", new URL(dashboard));
  vi.stubEnv("VITE_DEV_BACKEND_PORT", "8091");
  const { rerender, container } = render(LocalhostPreviewPanel, {
    props: { access: testDevice.access, target: parseLocalhostPreviewTarget(href) },
  });
  expect(screen.getByRole("alert")).toBeVisible();
  expect(container.querySelector("iframe")).toBeNull();

  const app = "http://localhost:3000/app";
  await rerender({ target: parseLocalhostPreviewTarget(app) });
  const frame = screen.getByTitle("Open preview");
  expect(frame).toHaveAttribute("src", app);
  expect(frame).not.toHaveAttribute("sandbox");
});

test("opens a remote device preview even when its port matches the Dashboard", () => {
  vi.stubGlobal("location", new URL("http://localhost:5173"));
  const target = parseLocalhostPreviewTarget("http://localhost:5173/app");
  const openLocalhostPreview = vi.fn();
  render(LocalhostPreviewPanel, {
    props: { access: { ...testDevice.access, openLocalhostPreview }, target },
  });
  expect(openLocalhostPreview).toHaveBeenCalledWith(target, screen.getByTitle("Open preview"));
});
