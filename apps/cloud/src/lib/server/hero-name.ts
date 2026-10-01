import { count, eq } from "drizzle-orm";
import type { Database } from "./db";
import { heroNameHeroes, heroNameModifiers } from "./db/schema";

const DNS_LABEL = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;
const HERO_NAME = /^([a-z0-9]+)-([a-z0-9]+)$/;
const MAX_DNS_LABEL_LENGTH = 63;
const UINT32_RANGE = 0x1_0000_0000;

declare const heroNameBrand: unique symbol;
export type HeroName = string & { readonly [heroNameBrand]: true };

function randomIndex(length: number): number {
  if (!Number.isSafeInteger(length) || length <= 0 || length > UINT32_RANGE) {
    throw new Error("Hero name vocabulary is empty or too large");
  }
  const unbiasedLimit = UINT32_RANGE - (UINT32_RANGE % length);
  const values = new Uint32Array(1);
  do {
    crypto.getRandomValues(values);
  } while (values[0] >= unbiasedLimit);
  return values[0] % length;
}

function parseHeroName(value: unknown): [modifier: string, hero: string] | null {
  if (typeof value !== "string" || value.length > MAX_DNS_LABEL_LENGTH || !DNS_LABEL.test(value)) {
    return null;
  }
  const match = HERO_NAME.exec(value);
  return match ? [match[1], match[2]] : null;
}

export async function generateHeroName(db: Database): Promise<HeroName> {
  const [modifierTotal, heroTotal] = await Promise.all([
    db.select({ value: count() }).from(heroNameModifiers).get(),
    db.select({ value: count() }).from(heroNameHeroes).get(),
  ]);
  const [modifier, hero] = await Promise.all([
    db
      .select({ word: heroNameModifiers.word })
      .from(heroNameModifiers)
      .orderBy(heroNameModifiers.word)
      .limit(1)
      .offset(randomIndex(modifierTotal?.value ?? 0))
      .get(),
    db
      .select({ word: heroNameHeroes.word })
      .from(heroNameHeroes)
      .orderBy(heroNameHeroes.word)
      .limit(1)
      .offset(randomIndex(heroTotal?.value ?? 0))
      .get(),
  ]);
  const name = `${modifier?.word ?? ""}-${hero?.word ?? ""}`;
  if (!(await isHeroName(db, name))) {
    throw new Error("Hero name vocabulary produced an invalid name");
  }
  return name as HeroName;
}

export async function isHeroName(db: Database, value: unknown): Promise<boolean> {
  const parts = parseHeroName(value);
  if (!parts) return false;
  const [modifier, hero] = parts;
  const [knownModifier, knownHero] = await Promise.all([
    db
      .select({ word: heroNameModifiers.word })
      .from(heroNameModifiers)
      .where(eq(heroNameModifiers.word, modifier))
      .get(),
    db
      .select({ word: heroNameHeroes.word })
      .from(heroNameHeroes)
      .where(eq(heroNameHeroes.word, hero))
      .get(),
  ]);
  return knownModifier !== undefined && knownHero !== undefined;
}
