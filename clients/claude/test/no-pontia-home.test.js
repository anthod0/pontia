import { describe, expect, test, vi } from "vitest";

import { runClaudeHook } from "../src/hook.js";

describe("Claude hook Pontia home boundary", () => {
    test.each([
        ["missing", undefined],
        ["empty", ""],
        ["relative", "relative/pontia"],
        ["tilde-prefixed", "~/.pontia"],
    ])("stays inactive when PONTIA_HOME is %s", async (_name, pontiaHome) => {
        const logDiagnostic = vi.fn();
        await runClaudeHook(
            { hook_event_name: "SessionStart" },
            {
                env: {
                    TMUX: "/tmp/tmux-test/default,1,0",
                    TMUX_PANE: "%1",
                    ...(pontiaHome === undefined ? {} : { PONTIA_HOME: pontiaHome }),
                },
                logDiagnostic,
            },
        );

        expect(logDiagnostic).not.toHaveBeenCalled();
    });
});
