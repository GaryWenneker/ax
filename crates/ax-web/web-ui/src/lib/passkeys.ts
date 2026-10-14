/** WebAuthn browser serialization. Private keys never enter JavaScript. */
function decode(value: string): ArrayBuffer {
  const raw = atob(value.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - value.length % 4) % 4));
  const bytes = Uint8Array.from(raw, char => char.charCodeAt(0));
  return bytes.buffer;
}
function encode(buffer: ArrayBuffer): string {
  let binary = '';
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
async function requestJson<T>(url: string, body: unknown): Promise<T> {
  const response = await fetch(url, { method: 'POST', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? 'Passkey request failed');
  return data as T;
}
type Ceremony = { ceremony: string; options: { publicKey: Record<string, unknown> } };
export async function registerPasskey(): Promise<void> {
  if (!window.PublicKeyCredential) throw new Error('Passkeys are not supported in this browser');
  const start = await requestJson<Ceremony>('/api/web-auth/passkeys/registration/start', {});
  const options = start.options.publicKey;
  const publicKey = {
    ...options,
    challenge: decode(options.challenge as string),
    user: { ...(options.user as object), id: decode((options.user as { id: string }).id) },
    excludeCredentials: ((options.excludeCredentials ?? []) as { id: string }[])
      .map(item => ({ ...item, id: decode(item.id) })),
  } as PublicKeyCredentialCreationOptions;
  const credential = await navigator.credentials.create({ publicKey }) as PublicKeyCredential | null;
  if (!credential) throw new Error('Passkey registration cancelled');
  const response = credential.response as AuthenticatorAttestationResponse;
  await requestJson('/api/web-auth/passkeys/registration/finish', {
    ceremony: start.ceremony,
    credential: {
      id: credential.id, rawId: encode(credential.rawId), type: credential.type,
      response: {
        clientDataJSON: encode(response.clientDataJSON),
        attestationObject: encode(response.attestationObject),
        transports: typeof response.getTransports === 'function' ? response.getTransports() : [],
      },
      clientExtensionResults: credential.getClientExtensionResults(),
    },
  });
}
export async function signInPasskey(): Promise<void> {
  if (!window.PublicKeyCredential) throw new Error('Passkeys are not supported in this browser');
  const start = await requestJson<Ceremony>('/api/web-auth/passkeys/authentication/start', {});
  const options = start.options.publicKey;
  const listed = (options.allowCredentials ?? []) as { type: PublicKeyCredentialType; id: string; transports?: AuthenticatorTransport[] }[];
  const publicKey = {
    ...options,
    challenge: decode(options.challenge as string),
  } as PublicKeyCredentialRequestOptions;
  if (listed.length === 0) delete publicKey.allowCredentials;
  else publicKey.allowCredentials = listed.map(item => ({ ...item, id: decode(item.id) }));
  const credential = await navigator.credentials.get({ publicKey }) as PublicKeyCredential | null;
  if (!credential) throw new Error('Passkey sign-in cancelled');
  const response = credential.response as AuthenticatorAssertionResponse;
  await requestJson('/api/web-auth/passkeys/authentication/finish', {
    ceremony: start.ceremony,
    credential: {
      id: credential.id, rawId: encode(credential.rawId), type: credential.type,
      response: {
        clientDataJSON: encode(response.clientDataJSON),
        authenticatorData: encode(response.authenticatorData),
        signature: encode(response.signature),
        userHandle: response.userHandle ? encode(response.userHandle) : null,
      },
      clientExtensionResults: credential.getClientExtensionResults(),
    },
  });
}
