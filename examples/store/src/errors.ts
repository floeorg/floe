import { type ApiError, Display } from "./types";

export function display(self: ApiError): string {
  return self.tag === "Network" && self.value.tag === "Timeout" ? (() => { const ms = self.value.ms; return `Request timed out after ${ms}ms`; })() : self.tag === "Network" && self.value.tag === "DnsFailure" ? (() => { const host = self.value.host; return `Cannot resolve ${host}`; })() : self.tag === "Network" && self.value.tag === "ConnectionRefused" ? "Server is not responding" : self.tag === "NotFound" ? (() => { const id = self.id; return `Product #${id} not found`; })() : self.tag === "BadResponse" ? (() => { const status = self.status; return (status >= 400 && status <= 499) ? `Client error (${status})` : (status >= 500 && status <= 599) ? `Server error (${status})` : (() => { const s = status; return `Unexpected status ${s}`; })(); })() : self.tag === "ParseError" ? (() => { const msg = self.message; return `Invalid response: ${msg}`; })() : (() => { throw new Error("non-exhaustive match"); })();
}

export function isRetryable(self: ApiError): boolean {
  return self.tag === "Network" ? true : self.tag === "BadResponse" ? (() => { const status = self.status; return status === 429 ? true : (status >= 500 && status <= 599) ? true : false; })() : false;
}


