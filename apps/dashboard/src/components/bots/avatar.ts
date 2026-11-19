import type { BotAvatarColor, BotAvatarShape } from "../../api/types";

export const avatarShapes = {
  classic:
    "M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z",
  mochi: "M51 30C72 30 84 44 88 62C91 74 92 84 80 84H22C10 84 11 74 14 62C18 44 30 30 51 30Z",
  cloud:
    "M18 72C8 62 12 46 22 42C22 30 36 26 44 32C48 24 62 24 66 32C74 26 88 32 84 44C94 50 92 66 82 74C68 86 32 86 18 72Z",
  drop: "M51 18C60 32 87 42 87 64C87 81 70 84 51 84C32 84 15 81 15 64C15 42 42 32 51 18Z",
  blob: "M24 34C34 22 52 30 62 26C76 20 92 34 86 52C82 64 92 76 78 82C64 88 50 78 36 84C20 90 10 76 14 62C17 52 15 44 24 34Z",
  ghost: "M18 50C18 32 32 24 51 24C70 24 84 32 84 50V76Q73 90 62 79Q51 90 40 79Q29 90 18 76Z",
  onigiri: "M51.0 32.0Q68.6 50.3 79.0 72.0Q51.0 76.0 23.0 72.0Q33.4 50.3 51.0 32.0Z",
  egg: "M51 18C70 18 84 44 84 62C84 78 70 86 51 86C32 86 18 78 18 62C18 44 32 18 51 18Z",
  pudding:
    "M30 28H72C78 28 80 31 81 36L88 76C89 82 86 84 80 84H22C16 84 13 82 14 76L21 36C22 31 24 28 30 28Z",
} satisfies Record<BotAvatarShape, string>;

export const avatarColors = {
  blue: "#458588",
  aqua: "#689d6a",
  green: "#98971a",
  yellow: "#d79921",
  orange: "#d65d0e",
  purple: "#b16286",
  indigo: "#5f6fa3",
  brown: "#9c6b3c",
  rose: "#c45a76",
} satisfies Record<BotAvatarColor, string>;

export const shapeOptions = Object.keys(avatarShapes) as BotAvatarShape[];
export const colorOptions = Object.keys(avatarColors) as BotAvatarColor[];

export function randomAvatar() {
  return {
    shape: shapeOptions[Math.floor(Math.random() * shapeOptions.length)],
    color: colorOptions[Math.floor(Math.random() * colorOptions.length)],
  };
}

export interface AvatarMorph {
  values: string;
  dur: string;
  keyTimes?: string;
  keySplines?: string;
}

export interface AvatarMotion {
  /** Working loop period in milliseconds. */
  period: number;
  /** Phases (fractions of the period) where the loop shows the still avatar. */
  rest: [number, number][];
  shape?: AvatarMorph;
  eyes?: AvatarMorph;
}

function morph(values: string[], keyTimes: number[], dur: string): AvatarMorph {
  return {
    values: values.join(";"),
    dur,
    keyTimes: keyTimes.join(";"),
    keySplines: Array(keyTimes.length - 1)
      .fill(".45 0 .55 1")
      .join(";"),
  };
}

// cloud: the three pleats are pinched one after another.
function cloudMorph() {
  const pleat = (p1: string, p2: string, p3: string) =>
    `M18 72C8 62 12 46 22 42C${p1}44 32C${p2}66 32C${p3}84 44C94 50 92 66 82 74C68 86 32 86 18 72Z`;
  const flat = ["22 30 36 26 ", "48 24 62 24 ", "74 26 88 32 "];
  const pinched = ["24 25 34 20 ", "50 17 60 17 ", "74 20 86 26 "];
  const pinch = (i: number) =>
    pleat(...(flat.map((f, j) => (j === i ? pinched[j] : f)) as [string, string, string]));
  const still = avatarShapes.cloud;
  return morph(
    [still, pinch(0), still, pinch(1), still, pinch(2), still, still],
    [0, 0.1, 0.22, 0.3, 0.42, 0.5, 0.62, 1],
    "2.4s",
  );
}

// drop: the tip sits at (x, y); (51, 18) is the still shape.
function dropMorph() {
  const tip = (x: number, y: number) =>
    `M${x} ${y}C${x + 9} ${y + 14} 87 42 87 64C87 81 70 84 51 84C32 84 15 81 15 64C15 42 ${x - 9} ${y + 14} ${x} ${y}Z`;
  const hop = [
    [0, 51, 18],
    [0.1, 51, 13],
    [0.22, 46, 23],
    [0.34, 53, 16],
    [0.44, 51, 18],
    [0.5, 51, 18],
    [0.6, 51, 13],
    [0.72, 56, 23],
    [0.84, 49, 16],
    [0.94, 51, 18],
    [1, 51, 18],
  ];
  return morph(
    hop.map(([, x, y]) => tip(x, y)),
    hop.map(([t]) => t),
    "2.4s",
  );
}

// blob: a wave of amplitude 1.5 travels along the outline while the eyes circle with a radius
// that grows from 0 to 2.5 and back, so both return to rest every 1.5 seconds.
function blobMorphs() {
  const nums = avatarShapes.blob.match(/-?[\d.]+/g)!.map(Number);
  const points: [number, number][] = [];
  for (let i = 0; i < nums.length; i += 2) points.push([nums[i], nums[i + 1]]);
  const frame = (t: number) => {
    const moved = points.map(([x, y]) => {
      const a = Math.atan2(y - 56, x - 51);
      const d = 1.5 * (Math.sin(3 * a - 2 * Math.PI * t) - Math.sin(3 * a));
      return [x + Math.cos(a) * d, y + Math.sin(a) * d].map((v) => +v.toFixed(2));
    });
    let path = `M${moved[0].join(" ")}`;
    for (let i = 1; i < moved.length; i += 3) {
      path += `C${moved[i].join(" ")} ${moved[i + 1].join(" ")} ${moved[i + 2].join(" ")}`;
    }
    return path + "Z";
  };
  const frames = 24;
  const steps = Array.from({ length: frames + 1 }, (_, i) => i / frames);
  return {
    shape: { values: steps.map(frame).join(";"), dur: "1.5s" },
    eyes: {
      values: steps
        .map((t) => {
          const r = 2.5 * Math.sin(Math.PI * t);
          const a = 2 * Math.PI * t;
          return `${(-r * Math.sin(a)).toFixed(2)} ${(-r * Math.cos(a)).toFixed(2)}`;
        })
        .join(";"),
      dur: "1.5s",
    },
  };
}

const blob = blobMorphs();

export const avatarMotions = {
  classic: { period: 1600, rest: [[0, 0.06]] },
  mochi: { period: 1800, rest: [[0.86, 1]] },
  cloud: { period: 2400, rest: [[0.66, 1]], shape: cloudMorph() },
  drop: {
    period: 2400,
    rest: [
      [0.44, 0.5],
      [0.94, 1],
    ],
    shape: dropMorph(),
  },
  blob: { period: 1500, rest: [[0, 0]], shape: blob.shape, eyes: blob.eyes },
  ghost: { period: 3200, rest: [[0.8, 1]] },
  onigiri: {
    period: 1600,
    rest: [
      [0, 0.04],
      [0.5, 0.54],
    ],
  },
  egg: { period: 2400, rest: [[0.34, 1]] },
  pudding: { period: 2000, rest: [[0.6, 1]] },
} satisfies Record<BotAvatarShape, AvatarMotion>;
