import { manageEdgePreview } from "$lib/server/edge-preview";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = (event) => manageEdgePreview(event, "provision");
