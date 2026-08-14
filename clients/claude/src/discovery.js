import { readFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { optionalString } from "./internal-api.js";
export function pontiaHomeFromEnv(env = process.env) {
    const value = optionalString(env.PONTIA_HOME);
    return value && isAbsolute(value) ? value : undefined;
}
function normalizeBaseUrl(value) {
    const trimmed = value.trim().replace(/\/+$/, "");
    if (/^https?:\/\//.test(trimmed))
        return trimmed;
    const bracketMatch = trimmed.match(/^\[([^\]]+)\]:(\d+)$/);
    const plainMatch = trimmed.match(/^([^:]+):(\d+)$/);
    const host = bracketMatch?.[1] ?? plainMatch?.[1];
    const port = bracketMatch?.[2] ?? plainMatch?.[2];
    if (host && port) {
        const localHost = host === "0.0.0.0" || host === "::" || host === "[::]" ? "127.0.0.1" : host;
        return port === "80" ? `http://${localHost}` : `http://${localHost}:${port}`;
    }
    return `http://${trimmed}`;
}
function parseConfigValue(config, key) {
    const match = config.match(new RegExp(`^\\s*${key}\\s*=\\s*\"([^\"]+)\"`, "m"));
    return match?.[1];
}
export async function resolvePontiaConnection(options = {}) {
    const env = options.env ?? process.env;
    const explicitInternal = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
    const explicitExternal = optionalString(env.PONTIA_EXTERNAL_API_URL);
    const explicitToken = optionalString(env.PONTIA_EXTERNAL_API_TOKEN);
    if (explicitInternal || explicitExternal) {
        const externalApiUrl = normalizeBaseUrl(explicitExternal ?? explicitInternal.replace(/\/internal\/v1\/events\/?$/, "/external/v1"));
        const internalEventUrl = explicitInternal ?? `${externalApiUrl.replace(/\/external\/v1\/?$/, "")}/internal/v1/events`;
        const internalBase = internalEventUrl.replace(/\/events\/?$/, "");
        return {
            internalEventUrl,
            externalApiUrl,
            externalApiToken: explicitToken,
            bindingUpsertUrl: `${internalBase}/runtime-bindings/upsert`,
        };
    }
    const pontiaHome = pontiaHomeFromEnv(env);
    if (!pontiaHome)
        return undefined;
    try {
        const config = await readFile(join(pontiaHome, "config.toml"), "utf8");
        const bindAddr = parseConfigValue(config, "bind_addr");
        if (!bindAddr)
            return undefined;
        const base = normalizeBaseUrl(bindAddr);
        return {
            internalEventUrl: `${base}/internal/v1/events`,
            externalApiUrl: `${base}/external/v1`,
            externalApiToken: explicitToken ?? parseConfigValue(config, "external_api_token"),
            bindingUpsertUrl: `${base}/internal/v1/runtime-bindings/upsert`,
        };
    }
    catch {
        return undefined;
    }
}
