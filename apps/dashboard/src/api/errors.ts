export class ApiError extends Error {
  constructor(
    message: string,
    readonly code: string = 'request_failed',
    readonly status: number = 0,
    readonly afterNetworkFailure: boolean = false,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}
