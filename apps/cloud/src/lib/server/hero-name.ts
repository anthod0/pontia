const PERSONALITIES = [
  "adaptable",
  "adventurous",
  "alert",
  "ardent",
  "attentive",
  "audacious",
  "balanced",
  "bold",
  "brave",
  "bright",
  "calm",
  "candid",
  "capable",
  "careful",
  "cheerful",
  "clever",
  "compassionate",
  "confident",
  "constant",
  "courteous",
  "curious",
  "daring",
  "decisive",
  "devoted",
  "diligent",
  "earnest",
  "faithful",
  "fearless",
  "gallant",
  "generous",
  "gentle",
  "gracious",
  "hardy",
  "helpful",
  "honest",
  "hopeful",
  "humble",
  "keen",
  "kind",
  "lively",
  "loyal",
  "noble",
  "patient",
  "prudent",
  "quick",
  "resolute",
  "resourceful",
  "serene",
  "silent",
  "sincere",
  "steadfast",
  "spirited",
  "stoic",
  "strong",
  "swift",
  "thoughtful",
  "tireless",
  "tranquil",
  "valiant",
  "vigilant",
  "warm",
  "wise",
  "witty",
  "zealous",
] as const;

const COLORS = [
  "amber",
  "azure",
  "beige",
  "black",
  "blue",
  "bronze",
  "brown",
  "cerulean",
  "coral",
  "crimson",
  "emerald",
  "gold",
  "gray",
  "green",
  "indigo",
  "ivory",
  "jade",
  "lavender",
  "lilac",
  "maroon",
  "ochre",
  "olive",
  "onyx",
  "orange",
  "purple",
  "red",
  "scarlet",
  "silver",
  "teal",
  "turquoise",
  "violet",
  "white",
] as const;

const HEROES = [
  "achilles",
  "aeneas",
  "ajax",
  "arjuna",
  "arthur",
  "atalanta",
  "atlas",
  "bedivere",
  "bellerophon",
  "beowulf",
  "bhima",
  "bors",
  "bradamante",
  "bran",
  "brunhild",
  "cadmus",
  "camilla",
  "castor",
  "cuchulainn",
  "deborah",
  "diomedes",
  "enkidu",
  "finn",
  "galahad",
  "gareth",
  "geraint",
  "gilgamesh",
  "gordafarid",
  "gawain",
  "hector",
  "heracles",
  "hippolyta",
  "horatius",
  "jason",
  "karna",
  "kay",
  "lancelot",
  "leonidas",
  "merlin",
  "nala",
  "nestor",
  "odysseus",
  "orion",
  "owain",
  "palamedes",
  "patroclus",
  "penthesilea",
  "perceval",
  "perseus",
  "pollux",
  "rama",
  "rhiannon",
  "roland",
  "rostam",
  "samson",
  "scathach",
  "siegfried",
  "sigurd",
  "sinbad",
  "starkad",
  "sundiata",
  "telemachus",
  "theseus",
  "tristan",
] as const;

const DNS_LABEL = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;
const MAX_DNS_LABEL_LENGTH = 63;
const UINT32_RANGE = 0x1_0000_0000;

const personalitySet = new Set<string>(PERSONALITIES);
const colorSet = new Set<string>(COLORS);
const heroSet = new Set<string>(HEROES);

export const HERO_NAME_COMBINATIONS = PERSONALITIES.length * COLORS.length * HEROES.length;

declare const heroNameBrand: unique symbol;
export type HeroName = string & { readonly [heroNameBrand]: true };

function randomIndex(length: number): number {
  const unbiasedLimit = UINT32_RANGE - (UINT32_RANGE % length);
  const values = new Uint32Array(1);
  do {
    crypto.getRandomValues(values);
  } while (values[0] >= unbiasedLimit);
  return values[0] % length;
}

export function generateHeroName(): HeroName {
  return [
    PERSONALITIES[randomIndex(PERSONALITIES.length)],
    COLORS[randomIndex(COLORS.length)],
    HEROES[randomIndex(HEROES.length)],
  ].join("-") as HeroName;
}

export function isHeroName(value: unknown): value is HeroName {
  if (typeof value !== "string" || value.length > MAX_DNS_LABEL_LENGTH || !DNS_LABEL.test(value)) {
    return false;
  }

  const [personality, color, hero, extra] = value.split("-");
  return (
    extra === undefined &&
    personalitySet.has(personality) &&
    colorSet.has(color) &&
    heroSet.has(hero)
  );
}
