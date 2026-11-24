import { MediaQuery } from "svelte/reactivity";

const landscapePhone = new MediaQuery("(max-height: 499px) and (pointer: coarse)");
const narrow = new MediaQuery("(width < 768px)");

export function isLandscapePhone(): boolean {
  return landscapePhone.current;
}

export function isMobileShell(): boolean {
  return narrow.current || isLandscapePhone();
}
