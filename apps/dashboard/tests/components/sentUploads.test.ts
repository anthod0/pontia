import { render, screen, within } from "@testing-library/svelte";
import { expect, test } from "vitest";
import UploadedFiles from "../../src/components/chat/files/UploadedFiles.svelte";
import SentSecrets from "../../src/components/chat/SentSecrets.svelte";

const uploadRoot = "/home/user/.pontia/state/uploads/u1/files";

test("a single upload shows its name with the extension kept apart and its type, without actions", () => {
  const name = "a-very-long-quarterly-report-name-that-overflows.final.pdf";
  const { container } = render(UploadedFiles, {
    files: [{ path: `${uploadRoot}/${name}`, name }],
  });
  const card = container.querySelector("[data-user-message-uploads]")!;
  expect(card).toHaveClass("rounded-2xl", "bg-muted");
  expect(card.querySelector(".truncate")).toHaveTextContent(
    "a-very-long-quarterly-report-name-that-overflows.final",
  );
  expect(within(card as HTMLElement).getByText(".pdf")).toHaveClass("shrink-0");
  expect(within(card as HTMLElement).getByText("PDF")).toBeInTheDocument();
  expect(card.querySelector(`[title="${uploadRoot}/${name}"]`)).not.toBeNull();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  expect(screen.queryByRole("link")).not.toBeInTheDocument();
});

test("several uploads share one list card with a file count", () => {
  const { container } = render(UploadedFiles, {
    files: [
      { path: `${uploadRoot}/notes.md`, name: "notes.md" },
      { path: `${uploadRoot}/Makefile`, name: "Makefile" },
    ],
  });
  const card = container.querySelector("[data-user-message-uploads]") as HTMLElement;
  expect(within(card).getByText("2 files")).toBeInTheDocument();
  expect(within(card).getAllByRole("listitem")).toHaveLength(2);
  expect(within(card).getByText("Makefile")).toBeInTheDocument();
  expect(within(card).getByText("File")).toBeInTheDocument();
});

test("sent secrets show as capsules with their label and masked value", () => {
  render(SentSecrets, {
    secrets: [
      { path: "/s/1", label: "OpenAI key", masked: "sk-…abcd" },
      { path: "/s/2", masked: "ghp_…wxyz" },
    ],
  });
  const items = screen.getAllByRole("listitem");
  expect(items[0]).toHaveClass("h-7", "rounded-full", "bg-muted");
  expect(within(items[0]).getByText("OpenAI key")).toHaveClass("font-medium");
  expect(within(items[0]).getByText("sk-…abcd")).toHaveClass("truncate", "font-mono");
  expect(within(items[1]).getByText("ghp_…wxyz")).toBeInTheDocument();
});
