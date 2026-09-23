export interface SessionContext {
  sessionId: string;
  clientType: "pi";
  runtimeInstanceId: string;
  clientSessionKey?: string;
  clientSessionFile?: string;
  clientSessionDir?: string;
  clientCwd?: string;
}
