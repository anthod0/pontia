import type { ParamMatcher } from "@sveltejs/kit";

// Let static device pages win for /bots/sessions, etc. when the public handle is "bots".
export const match: ParamMatcher = (value) => value === "bots";
