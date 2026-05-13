export class ApiError extends Error {
  constructor(
    message: string,
    readonly code: string = 'request_failed',
    readonly status: number = 0,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}
