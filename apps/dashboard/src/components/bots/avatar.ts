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
