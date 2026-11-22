import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SessionConversation from "../../src/lib/components/session-chat/SessionConversation.svelte";
import { token } from "../../src/stores/auth";

const reply = (id: string, turnId: string, ...names: string[]) => ({
  id,
  turnId,
  role: "assistant" as const,
  content: ["```attachments", ...names.map((name) => `/tmp/${name}.png`), "```"].join("\n"),
  status: "sent" as const,
  createdAt: "",
});

class VisibleObserver {
  constructor(private readonly callback: IntersectionObserverCallback) {}
  observe(target: Element) {
    this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
  }
  disconnect() {}
}

beforeEach(() => {
  token.set("viewer-test-token");
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:image");
  vi.stubGlobal(
    "fetch",
    vi.fn(() =>
      Promise.resolve(
        new Response("png", { headers: { "Content-Type": "image/png", "Content-Length": "3" } }),
      ),
    ),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

test("pages only between the images of the card that was opened", async () => {
  render(SessionConversation, {
    sessionId: "viewer-session",
    // img1 and img2 are one card; img3 and img4 are cards of their own.
    messages: [
      { id: "u1", role: "user", content: "first", status: "sent", turnId: "t1", createdAt: "" },
      reply("a1", "t1", "img1", "img2"),
      reply("a2", "t1", "img3"),
      { id: "u2", role: "user", content: "second", status: "sent", turnId: "t2", createdAt: "" },
      reply("a3", "t2", "img4"),
    ],
  });

  await fireEvent.click(await screen.findByRole("button", { name: "Enlarge img2.png" }));
  const viewer = await screen.findByRole("dialog");
  expect(
    within(viewer).getByRole("heading", { name: "img2.png", hidden: true }),
  ).toBeInTheDocument();
  const next = within(viewer).getByRole("button", { name: "Next image" });
  expect(next).toBeDisabled();
  await fireEvent.click(within(viewer).getByRole("button", { name: "Previous image" }));
  expect(
    within(viewer).getByRole("heading", { name: "img1.png", hidden: true }),
  ).toBeInTheDocument();
  expect(within(viewer).getByRole("button", { name: "Previous image" })).toBeDisabled();
  await fireEvent.click(within(viewer).getByRole("button", { name: "Close", exact: true }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

  await fireEvent.click(screen.getByRole("button", { name: "Enlarge img3.png" }));
  const single = await screen.findByRole("dialog");
  expect(
    within(single).getByRole("heading", { name: "img3.png", hidden: true }),
  ).toBeInTheDocument();
  expect(within(single).queryByRole("button", { name: "Next image" })).not.toBeInTheDocument();
  expect(within(single).queryByRole("button", { name: "Previous image" })).not.toBeInTheDocument();
});
