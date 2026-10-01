export type DeploymentLogLevel = "info" | "warn" | "error";

export interface DeploymentLogger {
  info(value: Record<string, unknown>): void;
  warn(value: Record<string, unknown>): void;
  error(value: Record<string, unknown>): void;
}

const consoleLogger: DeploymentLogger = {
  info: (value) => console.info(value),
  warn: (value) => console.warn(value),
  error: (value) => console.error(value),
};

export function logDeploymentEvent(
  level: DeploymentLogLevel,
  value: Record<string, unknown>,
  logger: DeploymentLogger = consoleLogger,
) {
  try {
    logger[level](value);
  } catch {
    // Diagnostics must not affect the deployment result.
  }
}
