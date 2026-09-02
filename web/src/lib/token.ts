// The daemon's token. `e120 ui` opens the app at `/#token=...`; the app takes
// it from the fragment once, keeps it for the tab in sessionStorage, and
// removes it from the address bar. Nothing here touches the DOM at import
// time, so the fragment logic is testable in node.

const KEY = "e120.token";

/** Split `token=...` out of a hash: the token and the hash without it. */
export function splitFragment(hash: string): { token: string | null; rest: string } {
  const m = /(^#|&)token=([^&]*)/.exec(hash);
  if (!m || !m[2]) return { token: null, rest: hash };
  const rest = hash.slice(0, m.index) + hash.slice(m.index + m[0].length);
  return { token: decodeURIComponent(m[2]), rest: rest === "#" ? "" : rest };
}

/** The token this tab has, or null. */
export function storedToken(): string | null {
  try {
    return sessionStorage.getItem(KEY);
  } catch {
    return null;
  }
}

/** Keep the token for this tab; an empty string forgets it. */
export function setToken(token: string) {
  try {
    if (token) sessionStorage.setItem(KEY, token);
    else sessionStorage.removeItem(KEY);
  } catch {
    /* no storage: the token lives only in memory */
  }
}

/**
 * The token to use at load: the fragment's, which is then stored and removed
 * from the address bar, else the stored one.
 */
export function loadToken(): string | null {
  const { token, rest } = splitFragment(location.hash);
  if (token) {
    setToken(token);
    history.replaceState(null, "", location.pathname + location.search + rest);
    return token;
  }
  return storedToken();
}
