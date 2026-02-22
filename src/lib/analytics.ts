/**
 * Thin wrapper around analytics tracking.
 * Aptabase removed for Windows ARM64 compatibility — calls are no-ops.
 */
export function track(
  _event: string,
  _props?: Record<string, string | number>,
) {
  // no-op: aptabase plugin removed
}
