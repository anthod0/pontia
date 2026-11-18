import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SessionConversation from "../../src/lib/components/session-chat/SessionConversation.svelte";
import { token } from "../../src/stores/auth";

const image = (id: string, turn: string) => ({
  file_id: id,
  turn_id: turn,
  name: `${id}.png`,
  path: `/tmp/${id}.png`,
  size: 10,
  media_type: "image/png",
  created_at: "2026-11-17T00:00:00Z",
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
    vi.fn(() => Promise.resolve(new Response("png", { headers: { "Content-Type": "image/png" } }))),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

test("pages between the shared images of every turn in conversation order", async () => {
  render(SessionConversation, {
    sessionId: "viewer-session",
    messages: [
      { id: "u1", role: "user", content: "first", status: "complete", turnId: "t1" },
      { id: "u2", role: "user", content: "second", status: "complete", turnId: "t2" },
    ],
    // The store lists newest first.
    sharedFiles: [image("img2", "t2"), image("img1", "t1")],
  });

  await fireEvent.click(await screen.findByRole("button", { name: "Enlarge img1.png" }));
  const viewer = await screen.findByRole("dialog");
  const next = within(viewer).getByRole("button", { name: "Next image" });
  expect(within(viewer).getByRole("button", { name: "Previous image" })).toBeDisabled();
  expect(within(viewer).getByText("1 / 2")).toBeInTheDocument();

  await fireEvent.click(next);
  await waitFor(() => expect(within(viewer).getByText("2 / 2")).toBeInTheDocument());
  expect(
    within(viewer).getByRole("heading", { name: "img2.png", hidden: true }),
  ).toBeInTheDocument();
  expect(next).toBeDisabled();
});
