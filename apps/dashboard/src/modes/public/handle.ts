import type { ParamMatcher } from "@sveltejs/kit";
import { isValidDeviceHandle } from "$lib/deviceHandle";

export const match: ParamMatcher = isValidDeviceHandle;
