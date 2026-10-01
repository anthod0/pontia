import { expect, test } from "bun:test";
import { generateHeroName, isHeroName } from "../src/lib/server/hero-name";
import { testDatabase } from "./database";

const database = testDatabase();

test("generates a DNS-safe two-part name from the database vocabulary", async () => {
  const name = await generateHeroName(database.db);

  expect(await isHeroName(database.db, name)).toBe(true);
  expect(name).toMatch(/^[a-z0-9]+-[a-z0-9]+$/);
  expect(name.length).toBeLessThanOrEqual(63);
});

test("accepts only canonical names from the database vocabulary", async () => {
  expect(await isHeroName(database.db, "brave-atlas")).toBe(true);

  for (const invalid of [
    null,
    "",
    "brave",
    "brave-silver-atlas",
    "Brave-atlas",
    "reckless-atlas",
    "brave-batman",
    `brave-${"a".repeat(64)}`,
  ]) {
    expect(await isHeroName(database.db, invalid)).toBe(false);
  }
});
