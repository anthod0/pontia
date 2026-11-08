import { mocks, session, timelineStateValue } from "../pages/chat/fixtures";
import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import SessionChatPage from "../../src/pages/SessionChatPage.svelte";

test("leaving a device's real chat page preserves its timeline and selection through popstate and unmount", async () => {
  window.history.replaceState({}, "", "/office-mac/chat/same");
  const selected = session({ session_id: "same" });
  mocks.loadedSessions = [selected];
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  const view = render(SessionChatPage);
  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith("same"));
  await waitFor(() => expect(mocks.timelineState.get().status).toBe("empty"));
  const cached = timelineStateValue({
    sessionId: "same",
    status: "ready",
    latestTurnId: "saved-turn",
  });
  mocks.timelineState.set(cached);
  vi.mocked(testDevice.sessions.selectSession).mockClear();
  window.history.pushState({}, "", "/travel-laptop");
  await fireEvent(window, new PopStateEvent("popstate"));
  view.unmount();
  expect(mocks.timelineState.get()).toEqual(cached);
  expect(testDevice.sessions.selectSession).not.toHaveBeenCalledWith(null);
  expect(mocks.dashboardEventListeners.size).toBe(0);
});

test("leaving chat within its own device still releases the active selection", async () => {
  window.history.replaceState({}, "", "/office-mac/chat/same");
  const view = render(SessionChatPage);
  window.history.pushState({}, "", "/office-mac/sessions");
  view.unmount();
  expect(testDevice.sessions.selectSession).toHaveBeenLastCalledWith(null);
  expect(mocks.timelineState.get().sessionId).toBe("");
});
