import { expect, test } from "bun:test";
import { resolveConfig } from "vite";
import { serializeJsonLd } from "../src/lib/site";

test.each(["cloud", "cron", "dashboard"])(
  "%s disables Workers development and preview URLs",
  async (app) => {
    const config = Bun.TOML.parse(
      await Bun.file(new URL(`../../${app}/wrangler.toml`, import.meta.url)).text(),
    );
    expect(config).toMatchObject({ workers_dev: false, preview_urls: false });
  },
);

test("Cloud form actions require a matching origin", async () => {
  const config = await resolveConfig({}, "serve");
  const kit = config.plugins.find((plugin) => plugin.name === "vite-plugin-sveltekit-setup");
  expect(kit?.api.options.kit.csrf).toMatchObject({ checkOrigin: true, trustedOrigins: [] });
});

test("JSON-LD preserves text without emitting HTML delimiters", () => {
  const data = { description: "Example </script> with <markup>" };
  const serialized = serializeJsonLd(data);
  expect(serialized).not.toContain("<");
  expect(JSON.parse(serialized)).toEqual(data);
});
