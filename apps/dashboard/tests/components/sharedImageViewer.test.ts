import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SessionConversation from "../../src/lib/components/session-chat/SessionConversation.svelte";
import { token } from "../../src/stores/auth";

const image = (id: string, turn: string, share = `share-${id}`) => ({
  file_id: id,
  share_id: share,
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

test("pages only between the images of the share that was opened", async () => {
  render(SessionConversation, {
    sessionId: "viewer-session",
    messages: [
      { id: "u1", role: "user", content: "first", status: "complete", turnId: "t1" },
      { id: "u2", role: "user", content: "second", status: "complete", turnId: "t2" },
    ],
    // The store lists newest first. img1 and img2 are one share; img3 and img4 are single shares.
    sharedFiles: [
      image("img4", "t2"),
      image("img3", "t1"),
      image("img2", "t1", "batch"),
      image("img1", "t1", "batch"),
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
