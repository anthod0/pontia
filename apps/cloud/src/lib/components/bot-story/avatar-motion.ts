import { shapes, type Shape } from "$lib/bot-stories";

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
  const nums = shapes.blob.match(/-?[\d.]+/g)!.map(Number);
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
  mochi: { period: 1800, rest: [[0.86, 1]] },
  drop: {
    period: 2400,
    rest: [
      [0.44, 0.5],
      [0.94, 1],
    ],
    shape: dropMorph(),
  },
  ghost: { period: 3200, rest: [[0.8, 1]] },
  blob: { period: 1500, rest: [[0, 0]], shape: blob.shape, eyes: blob.eyes },
  egg: { period: 2400, rest: [[0.34, 1]] },
  pudding: { period: 2000, rest: [[0.6, 1]] },
} satisfies Record<Shape, AvatarMotion>;
