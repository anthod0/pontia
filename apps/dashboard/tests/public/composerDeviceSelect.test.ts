import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import ComposerDeviceSelect from "../../src/modes/public/ComposerDeviceSelect.svelte";
import type { HostDevice } from "../../src/device/hostContext";

test("searches a long phone picker and selects the matching device", async () => {
  const matchMedia = window.matchMedia;
  const media = vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    ...matchMedia(query),
    matches: query.includes("max-width: 767px"),
  }));
  try {
    const devices: HostDevice[] = Array.from({ length: 10 }, (_, i) => ({
      handle: `device-${i}`,
      name: `Device ${i}`,
      id: String(i),
      runtime: null,
      state: "online",
    }));
    const onSelect = vi.fn();
    render(ComposerDeviceSelect, { devices, selected: devices[0], onSelect });
    await fireEvent.click(screen.getByRole("button", { name: "Device" }));
    const dialog = within(await screen.findByRole("dialog"));
    await fireEvent.input(dialog.getByRole("searchbox"), { target: { value: "DEVICE 9" } });
    expect(dialog.queryByRole("button", { name: /Device 0/ })).toBeNull();
    await fireEvent.click(dialog.getByRole("button", { name: /Device 9/ }));
    expect(onSelect).toHaveBeenCalledWith("device-9");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  } finally {
    media.mockRestore();
  }
});
