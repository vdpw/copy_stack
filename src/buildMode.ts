export interface ErrorPresentationPolicy {
  readonly listenForRuntimeOperationErrors: boolean;
  readonly showSafeDiagnosticDetails: boolean;
}

export function errorPresentationPolicy(
  isDevelopment: boolean
): ErrorPresentationPolicy {
  return {
    listenForRuntimeOperationErrors: isDevelopment,
    showSafeDiagnosticDetails: isDevelopment,
  };
}

export const currentErrorPresentationPolicy = errorPresentationPolicy(
  import.meta.env.DEV
);
