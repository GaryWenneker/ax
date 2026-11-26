// What the connection form needs before it may ask the server anything.
// The local server checks the same things again; this gives the message early.

export type AuthMode = 'api_key' | 'oauth';

export interface ConnectionInput {
  name: string;
  url: string;
  mode: AuthMode;
  key: string;
  clientId: string;
  write: boolean;
  /** Also ask for admin access (the management pages). */
  admin: boolean;
}

export const EMPTY_INPUT: ConnectionInput = { name: '', url: '', mode: 'api_key', key: '', clientId: '', write: false, admin: false };

function isLoopback(host: string): boolean {
  return host === 'localhost' || host === '[::1]' || /^127(\.\d{1,3}){3}$/.test(host);
}

/** An error message, or null when the address can be used. */
export function addressError(value: string): string | null {
  const text = value.trim();
  if (!text) return 'Enter the address of the Ax environment.';
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return 'The address is not a valid URL.';
  }
  if (url.username || url.password || url.search || url.hash) {
    return 'Use an address without a user name, password, query or fragment.';
  }
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && isLoopback(url.hostname))) {
    return 'Use an https:// address (http:// is accepted for this computer only).';
  }
  if (!/^\/projects\/[A-Za-z0-9_-]{1,64}\/mcp$/.test(url.pathname)) {
    return 'Use the project MCP address, ending in /projects/<project>/mcp.';
  }
  return null;
}

export type InputField = 'url' | 'key' | 'clientId' | 'name';

export interface InputProblem {
  field: InputField;
  message: string;
}

/**
 * The first thing wrong with the form, and the field it belongs to.
 * With `keepStoredKey`, an empty key means "keep the saved one" (editing).
 */
export function inputProblem(input: ConnectionInput, keepStoredKey = false): InputProblem | null {
  const address = addressError(input.url);
  if (address) return { field: 'url', message: address };
  if (input.name.trim().length > 80) return { field: 'name', message: 'Use a name of at most 80 characters.' };
  if (input.mode === 'api_key') {
    const key = input.key.trim();
    if (!key) return keepStoredKey ? null : { field: 'key', message: 'Enter the API key for this environment.' };
    if (/\s/.test(key)) return { field: 'key', message: 'The key contains spaces.' };
    return null;
  }
  if (!input.clientId.trim()) {
    return { field: 'clientId', message: "Enter the public client ID from the environment's connection details." };
  }
  if (new URL(input.url.trim()).protocol !== 'https:') {
    return { field: 'url', message: 'Browser sign-in needs an https:// address.' };
  }
  return null;
}

export function inputError(input: ConnectionInput, keepStoredKey = false): string | null {
  return inputProblem(input, keepStoredKey)?.message ?? null;
}

/** "host / project", the name used when none is given. */
export function suggestedName(url: string): string {
  try {
    const parsed = new URL(url.trim());
    const project = parsed.pathname.split('/')[2] ?? '';
    return project ? `${parsed.hostname} / ${project}` : parsed.hostname;
  } catch {
    return '';
  }
}
