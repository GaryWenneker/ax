export type IssuedToken = { value: string; clientName: string };

/**
 * The token banner after a mutation returns `data`. A shown token cannot be
 * displayed again, so it stays until the user dismisses it or a new one replaces it.
 */
export function issuedTokenAfter(
  current: IssuedToken | null,
  data: { credential?: { token: string } },
  tokenClient: string | undefined,
): IssuedToken | null {
  if (data.credential && tokenClient) return { value: data.credential.token, clientName: tokenClient };
  return current;
}
