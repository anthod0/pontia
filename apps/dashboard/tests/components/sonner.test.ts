import { render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import { toast } from "svelte-sonner";
import { afterEach, expect, test, vi } from "vitest";
import Toaster from "../../src/lib/components/ui/sonner/sonner.svelte";

const { isMobileShell } = vi.hoisted(() => ({ isMobileShell: vi.fn(() => false) }));
vi.mock("../../src/lib/hooks/shellMode.svelte", () => ({ isMobileShell }));

afterEach(() => toast.dismiss());

test("shows desktop notifications with live-region roles by type", async () => {
  isMobileShell.mockReturnValue(false);
  render(Toaster);

  toast.error("Upload failed", { duration: Number.POSITIVE_INFINITY });
  toast.success("Saved", { duration: Number.POSITIVE_INFINITY });

  toast.info("Setup skipped");
  expect(await screen.findByText("Setup skipped")).toBeInTheDocument();

  const failed = await screen.findByText("Upload failed");
  expect(failed.closest("[data-sonner-toast]")).toHaveAttribute("role", "alert");
  await waitFor(() =>
    expect(screen.getByText("Saved").closest("[data-sonner-toast]")).toHaveAttribute(
      "role",
      "status",
    ),
  );
});

test("filters mobile confirmations and keeps warnings, errors and actions", async () => {
  isMobileShell.mockReturnValue(true);
  render(Toaster);

  toast.success("Copied");
  toast.info("Setup skipped on mobile", { description: "Setup details" });
  toast.warning("Offline");
  toast.error("Copy failed");
  const onClick = vi.fn();
  toast.success("Cancelled", { action: { label: "Undo", onClick } });
  toast.info("Popup blocked", { action: { label: "Open", onClick } });
  await tick();

  expect(screen.queryByText("Copied")).toBeNull();
  expect(screen.queryByText("Setup skipped on mobile")).toBeNull();
  expect(screen.getByText("Offline")).toBeInTheDocument();
  expect(screen.getByText("Copy failed")).toBeInTheDocument();
  screen.getByRole("button", { name: "Undo" }).click();
  screen.getByRole("button", { name: "Open" }).click();
  expect(onClick).toHaveBeenCalledTimes(2);
});
