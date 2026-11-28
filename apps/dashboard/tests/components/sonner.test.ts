import { render, screen, waitFor } from "@testing-library/svelte";
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
