import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { toast } from "svelte-sonner";
import { afterEach, expect, test, vi } from "vitest";
import Toaster from "../../src/lib/components/ui/sonner/sonner.svelte";

vi.mock("../../src/lib/hooks/shellMode.svelte", () => ({ isMobileShell: () => true }));

afterEach(() => toast.dismiss());

test("assigns live-region roles by notification type", async () => {
  render(Toaster);

  toast.error("Upload failed", { duration: Number.POSITIVE_INFINITY });
  toast.success("Saved", { duration: Number.POSITIVE_INFINITY });

  const failed = await screen.findByText("Upload failed");
  expect(failed.closest("[data-sonner-toast]")).toHaveAttribute("role", "alert");
  await waitFor(() =>
    expect(screen.getByText("Saved").closest("[data-sonner-toast]")).toHaveAttribute(
      "role",
      "status",
    ),
  );
});

test("expands a mobile stack on tap and collapses it on an outside tap", async () => {
  Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
    configurable: true,
    value: vi.fn(),
  });
  render(Toaster);
  toast.info("First", { duration: Number.POSITIVE_INFINITY });
  toast.success("Second", { duration: Number.POSITIVE_INFINITY });
  await screen.findByText("First");
  const front = await screen.findByText("Second");

  await fireEvent.pointerDown(front, { clientX: 100, clientY: 100 });
  await fireEvent.pointerUp(front, { clientX: 100, clientY: 100 });
  await waitFor(() =>
    expect(document.querySelectorAll('[data-sonner-toast][data-expanded="true"]')).toHaveLength(2),
  );

  await fireEvent.pointerDown(document.body, { clientX: 0, clientY: 300 });
  await waitFor(() =>
    expect(document.querySelectorAll('[data-sonner-toast][data-expanded="false"]')).toHaveLength(2),
  );
});
