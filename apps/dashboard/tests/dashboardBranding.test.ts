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

test("dashboard SVG favicon uses the small-size logo variant", () => {
  const head = new DOMParser().parseFromString(appHtml, "text/html").head;
  expect(head.querySelector('link[rel="icon"][type="image/svg+xml"]')?.getAttribute("href")).toBe(
    "%sveltekit.assets%/logo-small.svg",
  );
});

test.each(["logo.svg", "logo-small.svg"])("%s has a transparent background", (name) => {
  const svg = new DOMParser().parseFromString(
    readFileSync(resolve(__dirname, "../public", name), "utf8"),
    "image/svg+xml",
  );
  expect(svg.querySelector("#background")).toBeNull();
  expect(svg.querySelector(":root > rect")).toBeNull();
});

test.each(["logo.svg", "logo-small.svg"])("cloud %s matches the dashboard copy", (name) => {
  expect(readFileSync(resolve(__dirname, "../../cloud/static", name), "utf8")).toBe(
    readFileSync(resolve(__dirname, "../public", name), "utf8"),
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
