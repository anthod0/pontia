export interface SessionContext {
  sessionId: string;
  clientType: "pi";
  internalEventUrl: string;
  runtimeInstanceId: string;
  clientSessionKey?: string;
  clientSessionFile?: string;
  clientSessionDir?: string;
  clientCwd?: string;
}
