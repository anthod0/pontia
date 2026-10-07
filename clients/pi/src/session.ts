export interface SessionContext {
  sessionId: string;
  clientType: "pi";
  runtimeId: string;
  clientSessionKey?: string;
  clientSessionFile?: string;
  clientSessionDir?: string;
  clientCwd?: string;
}
