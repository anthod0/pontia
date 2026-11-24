import { expect, test } from "bun:test";
import { approve, deny, finish, send, start, type RemoteDemo } from "../src/lib/remote-demo";

const kinds = (demo: RemoteDemo) =>
  demo.entries.map((entry) => `${entry.kind}:${"id" in entry ? entry.id : entry.answer}`);

test("starts on the approval the agent is waiting for", () => {
  const demo = start();

  expect(demo.waiting).toEqual({ kind: "approval" });
  expect(demo.entries.at(-1)).toEqual({ kind: "ask", answer: null });
});

test("approving runs the tests, then offers a pull request and finishes once it is open", () => {
  const running = approve(start());
  expect(running.waiting).toEqual({ kind: "agent" });
  expect(running.entries.at(-1)).toEqual({ kind: "step", id: "tests-all", done: false });

  const passed = finish(running);
  expect(passed.waiting).toEqual({ kind: "message", suggestion: "pr" });

  const shipped = finish(send(passed, "pr"));
  expect(shipped.waiting).toEqual({ kind: "finished" });
  expect(kinds(shipped).slice(4)).toEqual([
    "ask:approve",
    "step:tests-all",
    "reply:passed",
    "prompt:pr",
    "step:pr",
    "reply:shipped",
  ]);
  expect(shipped.entries.every((entry) => entry.kind !== "step" || entry.done)).toBe(true);
});

test("denying lets the visitor steer the agent to a narrower test run", () => {
  const steering = deny(start());
  expect(steering.waiting).toEqual({ kind: "message", suggestion: "auth" });

  const passed = finish(send(steering, "auth"));
  expect(passed.waiting).toEqual({ kind: "message", suggestion: "pr" });
  expect(kinds(passed).slice(4)).toEqual([
    "ask:deny",
    "reply:instead",
    "prompt:auth",
    "step:tests-auth",
    "reply:passed",
  ]);
});

test("answers the approval once", () => {
  const demo = approve(start());

  expect(demo.entries.filter((entry) => entry.kind === "ask")).toEqual([
    { kind: "ask", answer: "approve" },
  ]);
});
