import { type ApiError, Display } from "./types";
export declare function display(self: ApiError): string;
export declare function isRetryable(self: ApiError): boolean;
