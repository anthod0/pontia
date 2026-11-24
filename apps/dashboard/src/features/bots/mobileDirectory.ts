import { createContext } from "svelte";
import type { createBotDirectory } from "./botDirectory.svelte";
export const [useMobileDirectory, provideMobileDirectory] = createContext<ReturnType<
  typeof createBotDirectory
> | null>();
