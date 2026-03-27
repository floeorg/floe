import type { ApiError } from "./types";

// ── Display for ApiError ────────────────────────────────

export function displayApiError(error: ApiError): string {
  switch (error.tag) {
    case "Network":
      switch (error.error.tag) {
        case "Timeout":
          return `Request timed out after ${error.error.ms}ms`;
        case "DnsFailure":
          return `Cannot resolve ${error.error.host}`;
        case "ConnectionRefused":
          return "Server is not responding";
      }
      break;
    case "NotFound":
      return `Product #${error.id} not found`;
    case "BadResponse":
      if (error.status >= 400 && error.status < 500) return `Client error (${error.status})`;
      if (error.status >= 500 && error.status < 600) return `Server error (${error.status})`;
      return `Unexpected status ${error.status}`;
    case "ParseError":
      return `Invalid response: ${error.message}`;
  }
}

export function isRetryable(error: ApiError): boolean {
  switch (error.tag) {
    case "Network":
      return true;
    case "BadResponse":
      return error.status === 429 || (error.status >= 500 && error.status < 600);
    default:
      return false;
  }
}
