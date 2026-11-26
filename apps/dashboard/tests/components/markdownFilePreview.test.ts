import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import MarkdownFilePreview from "../../src/components/chat/MarkdownFilePreview.svelte";

test("shows frontmatter separately from the rendered Markdown body", () => {
  const { container } = render(MarkdownFilePreview, {
    props: {
      content: "---\ntitle: Release notes\ndraft: true\n---\n# Changes\n\nReady to ship.",
    },
  });

  const disclosure = screen.getByText("Frontmatter").closest("details");
  expect(disclosure).not.toHaveAttribute("open");
  expect(disclosure?.querySelector("pre")?.textContent).toBe("title: Release notes\ndraft: true");
  expect(screen.getByRole("heading", { name: "Changes" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Release notes" })).not.toBeInTheDocument();
  expect(container.querySelector("hr")).not.toBeInTheDocument();
});

test("renders ordinary Markdown without a frontmatter disclosure", () => {
  render(MarkdownFilePreview, { props: { content: "# Notes\n\n---\n\nDetails" } });

  expect(screen.queryByText("Frontmatter")).not.toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Notes" })).toBeInTheDocument();
  expect(document.querySelector("hr")).toBeInTheDocument();
});
