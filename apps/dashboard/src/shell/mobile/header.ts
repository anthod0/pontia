import { createContext, type Snippet } from "svelte";
import { writable, type Writable } from "svelte/store";

type MobileHeader = {
  switchDevice: () => void;
  title: Writable<Snippet | null>;
  action: Writable<Snippet | null>;
};
export const [useMobileHeader, setMobileHeader] = createContext<MobileHeader>();

export function provideMobileHeader(switchDevice: () => void) {
  return setMobileHeader({ switchDevice, title: writable(null), action: writable(null) });
}
