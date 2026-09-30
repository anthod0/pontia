import { expect, test } from "bun:test";
import { generateHeroName, HERO_NAME_COMBINATIONS, isHeroName } from "../src/lib/server/hero-name";

test("provides a sufficiently large hero name space", () => {
  expect(HERO_NAME_COMBINATIONS).toBeGreaterThanOrEqual(100_000);
});

test("generates a DNS-safe three-part name with cryptographic randomness", () => {
  const name = generateHeroName();

  expect(isHeroName(name)).toBe(true);
  expect(name).toMatch(/^[a-z]+-[a-z]+-[a-z]+$/);
  expect(name.length).toBeLessThanOrEqual(63);
});

test("accepts only canonical names from the curated word lists", () => {
  expect(isHeroName("brave-silver-atlas")).toBe(true);

  for (const invalid of [
    null,
    "",
    "brave-silver",
    "brave-silver-atlas-extra",
    "Brave-silver-atlas",
    "reckless-silver-atlas",
    "brave-chartreuse-atlas",
    "brave-silver-batman",
    `brave-silver-${"a".repeat(64)}`,
  ]) {
    expect(isHeroName(invalid)).toBe(false);
  }
});
