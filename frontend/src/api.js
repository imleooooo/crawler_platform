/**
 * Thin fetch wrapper.  Authorization is injected by the nginx reverse proxy,
 * so no bearer token is ever included in the client bundle.
 */
export function apiFetch(path, options = {}) {
  return fetch(path, options);
}
