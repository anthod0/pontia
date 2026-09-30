import type { ParamMatcher } from "@sveltejs/kit";
import { isValidDeviceHandle } from "$lib/remoteDashboard";

export const match: ParamMatcher = isValidDeviceHandle;
