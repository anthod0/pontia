import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

const __dirname = dirname(fileURLToPath(import.meta.url));
const appHtml = readFileSync(resolve(__dirname, "../src/app.html"), "utf8");
const manifest = JSON.parse(
  readFileSync(resolve(__dirname, "../public/manifest.webmanifest"), "utf8"),
) as {
  id: string;
  start_url: string;
  scope: string;
  display: string;
  icons: Array<{ src: string; sizes: string; type: string }>;
};

test("dashboard head links the installable manifest", () => {
  const head = new DOMParser().parseFromString(appHtml, "text/html").head;
  expect(head.querySelector('link[rel="manifest"]')?.getAttribute("href")).toBe(
    "%sveltekit.assets%/manifest.webmanifest",
  );
});

test("dashboard manifest is installable within the dashboard scope", () => {
  expect(manifest).toMatchObject({
    id: "./",
    start_url: "./",
    scope: "./",
    display: "standalone",
  });
  expect(manifest.icons).toEqual(
    expect.arrayContaining([
      { src: "logo-192.png", sizes: "192x192", type: "image/png" },
      { src: "logo-512.png", sizes: "512x512", type: "image/png" },
    ]),
  );
});
