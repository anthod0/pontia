import { expect, test } from "vitest";
import { parseLocalhostPreviewTarget } from "../src/lib/localhostPreview";

test.each([
  ["http://LOCALHOST", 80, "/"],
  ["http://127.0.0.1:5173/page?q=1#section", 5173, "/page?q=1#section"],
  ["http://[::1]:8080", 8080, "/"],
  ["http://0.0.0.0:65535", 65535, "/"],
] as const)("recognizes %s", (href, port, redirect) => {
  expect(parseLocalhostPreviewTarget(href)).toEqual({ rawHref: href, port, redirect });
});

test.each([
  undefined,
  "/tmp/file.md",
  "https://localhost:8080",
  "http://example.com",
  "http://name:password@localhost",
  "http://localhost@evil.example",
  "http://localhost.example",
  "http://localhost:0",
  "http://localhost:65536",
  "http://127.1",
])("passes through %s", (href) => expect(parseLocalhostPreviewTarget(href)).toBeNull());
