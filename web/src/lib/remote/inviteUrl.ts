export interface InviteUrlWindowLike {
  readonly location: { readonly href: string };
  readonly history: {
    replaceState(data: unknown, unused: string, url?: string | URL | null): void;
  };
}

/**
 * Strips the `code` query parameter from a URL string while preserving pathname,
 * any other query parameters, and hash fragment.
 *
 * Returns the cleaned relative URL (`pathname + search + hash`) if `code` was present,
 * or `null` when the URL did not contain a `code` parameter (or could not be parsed).
 */
export function stripInviteCodeFromUrl(href: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(href);
  } catch {
    return null;
  }

  if (!parsed.searchParams.has("code")) {
    return null;
  }

  parsed.searchParams.delete("code");
  const search = parsed.searchParams.toString();
  return `${parsed.pathname}${search.length > 0 ? `?${search}` : ""}${parsed.hash}`;
}

/**
 * Removes any residual `?code=...` invite passcode parameter from the browser address bar
 * using `history.replaceState` without triggering a navigation or reload.
 */
export function clearInviteCodeFromLocation(
  win: InviteUrlWindowLike | null | undefined = typeof window !== "undefined"
    ? window
    : null,
): boolean {
  if (!win?.location?.href || !win.history?.replaceState) {
    return false;
  }
  const cleaned = stripInviteCodeFromUrl(win.location.href);
  if (cleaned === null) {
    return false;
  }
  try {
    win.history.replaceState(null, "", cleaned);
    return true;
  } catch {
    return false;
  }
}
