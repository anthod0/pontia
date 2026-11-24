import { createContext, type Snippet } from "svelte";
import type { Writable } from "svelte/store";
export const [useMobileTitle, provideMobileTitle] = createContext<Writable<Snippet | null>>();
