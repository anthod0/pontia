import { execFile } from "node:child_process";
import { realpath } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { expect, test } from "vitest";
import { tempDir } from "./temp-dir.js";

test("child commands inherit the current binding independently in two Pi processes", async () => {
  const root = await realpath(await tempDir("pi-env-"));
  const results = await Promise.all(
    ["one", "two"].map(async (suffix) => {
      const { stdout } = await promisify(execFile)(
        process.execPath,
        [
          "--experimental-transform-types",
          fileURLToPath(new URL("./session-environment.mjs", import.meta.url)),
          root,
          suffix,
        ],
        {
          env: {
            ...process.env,
            PONTIA_HOME: root,
            PONTIA_SESSION_ID: "inherited-wrong-session",
            PONTIA_RUNTIME_INSTANCE_ID: "inherited-wrong-runtime",
            TMUX: "",
            TMUX_PANE: "",
          },
        },
      );
      return { suffix, ...JSON.parse(stdout) };
    }),
  );
  for (const { suffix, observations, runtimes } of results) {
    expect(runtimes).toEqual(
      Object.fromEntries(
        Object.entries(observations).map(([step, session]) => [
          step,
          session === "unset" ? "unset" : `rt_${session}`,
        ]),
      ),
    );
    expect(observations).toEqual({
      unbound: "unset",
      created: `sess_${suffix}_1`,
      shutdown: "unset",
      restored: `sess_${suffix}_1`,
      reloading: "unset",
      reloaded: `sess_${suffix}_1`,
      switching: "unset",
      switched: `sess_${suffix}_2`,
      forked: `sess_${suffix}_3`,
      returned: `sess_${suffix}_1`,
      failed: "unset",
      deferred: "unset",
      manual: `sess_${suffix}_4`,
      inactive: "unset",
      duplicate: "unset",
      missing_reload: "unset",
      non_tui: "unset",
    });
  }
});
