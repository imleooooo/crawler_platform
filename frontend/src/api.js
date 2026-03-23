const BASE_URL = import.meta.env.VITE_API_URL ?? 'http://localhost:8000';
const API_KEY  = import.meta.env.VITE_API_KEY  ?? '';

/**
 * Wrapper around fetch that:
 *  - prepends VITE_API_URL to every path
 *  - injects Authorization: Bearer <VITE_API_KEY> on every request
 */
export function apiFetch(path, options = {}) {
  const { headers, ...rest } = options;
  return fetch(`${BASE_URL}${path}`, {
    ...rest,
    headers: {
      Authorization: `Bearer ${API_KEY}`,
      ...headers,
    },
  });
}
