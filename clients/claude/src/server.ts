import { randomUUID } from "node:crypto";
import {
  connectRuntime,
  CONTROL_VERSION,
  type RuntimeConnection,
} from "../shared/control-socket.js";
import { Host, ControlError, type QueryFactory, type ExecutionPolicy } from "./host.js";
import type { HostEvent } from "./facts.js";

export interface Bootstrap {
  session_id: string;
  runtime_id: string;
  pontia_home: string;
  cwd: string;
  model?: string;
  resume?: string;
  system_prompt?: string;
  execution_policy?: ExecutionPolicy;
}

type ObjectValue = Record<string, unknown>;
function object(value: unknown): ObjectValue {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new ControlError("rejected", "invalid_request");
  return value as ObjectValue;
}
function string(value: unknown): string {
  if (typeof value !== "string" || !value) throw new ControlError("rejected", "invalid_request");
  return value;
}

export function bootstrap(value: unknown): Bootstrap {
  const data = object(value);
  const result: Bootstrap = {
    session_id: string(data.session_id),
    runtime_id: string(data.runtime_id),
    pontia_home: string(data.pontia_home),
    cwd: string(data.cwd),
  };
  if (data.system_prompt !== undefined) result.system_prompt = string(data.system_prompt);
  if (data.model !== undefined) result.model = string(data.model);
  if (data.resume !== undefined) result.resume = string(data.resume);
  if (data.execution_policy !== undefined) {
    const policy = object(data.execution_policy);
    if (
      typeof policy.yolo !== "boolean" ||
      typeof policy.disable_mcp !== "boolean" ||
      typeof policy.disable_skills !== "boolean" ||
      !Array.isArray(policy.disabled_tools) ||
      policy.disabled_tools.some((tool) => typeof tool !== "string" || !tool.trim()) ||
      (policy.tools !== null &&
        (!Array.isArray(policy.tools) ||
          policy.tools.some((tool) => typeof tool !== "string" || !tool.trim())))
    )
      throw new ControlError("rejected", "invalid_execution_policy");
    result.execution_policy = {
      tools: policy.tools as ExecutionPolicy["tools"],
      disabled_tools: policy.disabled_tools as string[],
      disable_mcp: policy.disable_mcp,
      disable_skills: policy.disable_skills,
      yolo: policy.yolo,
    };
  }
  return result;
}

export async function serve(config: Bootstrap, factory?: QueryFactory) {
  const generation = randomUUID();
  let connected = false;
  let sequence = 0;
  let connection: RuntimeConnection;
  const reports = new Set<Promise<unknown>>();
  const identity = {
    sessionId: config.session_id,
    runtimeId: config.runtime_id,
    clientSessionKey: config.resume,
  };
  const host = new Host(
    config,
    (event: HostEvent) => {
      sequence++;
      if (event.kind === "ready") identity.clientSessionKey = event.native_session_id;
      if (!connected) return;
      const report = connection.request("host.event", { host_id: generation, sequence, event });
      reports.add(report);
      void report
        .catch(() => {
          console.error("Claude host fact delivery failed");
        })
        .finally(() => reports.delete(report));
      connection.registered(identity);
    },
    factory,
  );
  const invoke = async (request: ObjectValue): Promise<ObjectValue> => {
    try {
      switch (request.method) {
        case "input":
          host.submit(string(request.input_id), string(request.text));
          break;
        case "steer":
          host.steer(
            string(request.target_input_id),
            string(request.input_id),
            string(request.text),
          );
          break;
        case "compact":
          host.compact(string(request.operation_id));
          break;
        case "interrupt":
          await host.interrupt(string(request.input_id));
          break;
        case "permission":
          host.resolvePermission(string(request.permission_id), string(request.option));
          break;
        case "is_idle":
          return { status: "accepted", idle: host.isIdle() };
        case "close":
          await host.close();
          break;
        default:
          throw new ControlError("unsupported", "unsupported_method");
      }
      return { status: "accepted" };
    } catch (error) {
      return error instanceof ControlError
        ? { status: error.code, reason: error.reason }
        : { status: "unknown", reason: "operation_unconfirmed" };
    }
  };

  connection = await connectRuntime(
    config.pontia_home,
    () => console.error("Claude host control connection unavailable"),
    (method, params, respond, error) => {
      if (method === "host.status") {
        respond({
          result: {
            host_id: generation,
            ...host.status(),
            launch: {
              cwd: config.cwd,
              config_dir: process.env.CLAUDE_CONFIG_DIR,
              model: config.model,
              resume: config.resume,
            },
          },
        });
        return;
      }
      if (method !== "host.command") {
        error(-32601, "Unknown host method");
        return;
      }
      connected = true;
      host.controlRestored();
      void invoke(params)
        .then(async (result) => {
          if (params.method === "close" && result.status === "accepted") {
            await Promise.allSettled(reports);
            respond({ result }, () => {
              void connection.close();
            });
          } else respond({ result });
        })
        .catch(() => error(-32603, "Host control failed"));
    },
    async () => {
      connected = true;
      host.controlRestored();
    },
    () => {
      connected = false;
      host.controlLost();
    },
    true,
  );
  try {
    const attached = (await connection.request("runtime.attach", {
      version: CONTROL_VERSION,
      session_id: config.session_id,
      runtime_id: config.runtime_id,
      client_session_key: identity.clientSessionKey,
    })) as Record<string, unknown>;
    if (attached.session_id !== config.session_id || attached.runtime_id !== config.runtime_id)
      throw new Error("runtime_identity_mismatch");
    connection.registered(identity);
    connected = true;
    host.controlRestored();
  } catch (error) {
    await connection.close();
    throw error;
  }
  const terminate = () => {
    void close();
  };
  process.once("SIGTERM", terminate);
  async function close() {
    process.off("SIGTERM", terminate);
    try { await host.close(); }
    finally {
      await Promise.allSettled(reports);
      await connection.close();
    }
  }
  return { close };
}
