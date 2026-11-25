import cloudflare from "@sveltejs/adapter-cloudflare";
import type { Adapter } from "@sveltejs/kit";
import { appendFile, rename, writeFile } from "node:fs/promises";
import { relative } from "node:path";
import { securityHeaders } from "../src/lib/server/security-headers";

export function localeAdapter(): Adapter {
  const adapter = cloudflare();
  return {
    ...adapter,
    async adapt(builder) {
      await adapter.adapt(builder);
      const output = builder.getBuildDirectory("cloudflare");
      await rename(`${output}/_worker.js`, `${output}/_sveltekit.js`);
      const navigation = relative(output, "src/lib/server/locale-navigation.ts");
      const security = relative(output, "src/lib/server/security-headers.ts");
      await appendFile(
        `${output}/_headers`,
        "\n/*\n" +
          Object.entries(securityHeaders)
            .map(([key, value]) => `  ${key}: ${value}\n`)
            .join(""),
      );
      await writeFile(
        `${output}/_worker.js`,
        `import worker from "./_sveltekit.js";
import { localeNavigation } from ${JSON.stringify(navigation)};
import { secureResponse } from ${JSON.stringify(security)};
export default {
  async fetch(request, env, ctx) {
    return secureResponse(request, await localeNavigation(request, () => worker.fetch(request, env, ctx)));
  }
};
`,
      );
      await appendFile(`${output}/.assetsignore`, "\n_sveltekit.js\n");
    },
  };
}
