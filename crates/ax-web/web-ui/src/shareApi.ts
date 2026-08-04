const SHARE = '/api/policy/share';
const MS_AUTH = '/api/auth/microsoft';

export type ShareProvider = 'onedrive' | 'github';
export type ShareImportMode = 'review' | 'merge' | 'force';

export interface ShareContentConfig {
  rules: boolean;
  skills: boolean;
  memory: boolean;
}

export interface OneDriveShareConfig {
  shareUrl: string;
}

export interface GithubShareConfig {
  repoUrl: string;
  branch: string;
  subpath: string;
  /** Optional API token (e.g. GitLab PAT) for hosts that gate raw git behind SSO. */
  token: string;
}

export interface ShareConfig {
  provider: ShareProvider;
  content: ShareContentConfig;
  importMode: ShareImportMode;
  autoSyncMinutes: number;
  onedrive: OneDriveShareConfig;
  github: GithubShareConfig;
}

export interface ShareConfigResponse extends ShareConfig {
  configPath: string;
  scope: 'project';
}

export interface ShareSyncStatus {
  lastSyncAt?: number | null;
  lastError?: string | null;
  provider?: string | null;
  rulesAdded: number;
  skillsAdded: number;
  rulesPending: number;
  skillsPending: number;
  memoryInserted: number;
  memoryUpdated: number;
  remoteFiles: number;
}

export interface MicrosoftAuthStatus {
  signedIn: boolean;
  account?: string | null;
  expiresAt?: number | null;
  clientConfigured?: boolean;
  customClientId?: boolean;
}

export interface ShareStatusResponse {
  sync: ShareSyncStatus;
  microsoft: MicrosoftAuthStatus;
}

export interface DeviceFlowStart {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  verificationUriComplete?: string | null;
  expiresIn: number;
  interval: number;
  message: string;
}

export interface DevicePollResponse {
  complete: boolean;
  status: MicrosoftAuthStatus;
}

export type SyncDirection = 'pull' | 'push' | 'both';

interface ApiErrorBody {
  error?: string;
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
    ...init,
  });
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as ApiErrorBody;
    throw new Error(body.error ?? `HTTP ${res.status}`);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return res.json() as Promise<T>;
}

export function fetchShareConfig(): Promise<ShareConfigResponse> {
  return request(`${SHARE}/config`);
}

export function saveShareConfig(config: ShareConfig): Promise<ShareConfigResponse> {
  return request(`${SHARE}/config`, { method: 'PUT', body: JSON.stringify(config) });
}

export function fetchShareStatus(): Promise<ShareStatusResponse> {
  return request(`${SHARE}/status`);
}

export function runShareSync(direction: SyncDirection = 'pull'): Promise<ShareSyncStatus> {
  return request(`${SHARE}/sync`, {
    method: 'POST',
    body: JSON.stringify({ direction }),
  });
}

export function startMicrosoftDeviceFlow(): Promise<DeviceFlowStart> {
  return request(`${MS_AUTH}/device/start`, { method: 'POST', body: '{}' });
}

export function pollMicrosoftDeviceFlow(): Promise<DevicePollResponse> {
  return request(`${MS_AUTH}/device/poll`, { method: 'POST', body: '{}' });
}

export function fetchMicrosoftAuthStatus(): Promise<MicrosoftAuthStatus> {
  return request(`${MS_AUTH}/status`);
}

export function signOutMicrosoft(): Promise<void> {
  return request(`${MS_AUTH}`, { method: 'DELETE' });
}

export const DEFAULT_SHARE_CONFIG: ShareConfig = {
  provider: 'onedrive',
  content: { rules: true, skills: true, memory: false },
  importMode: 'review',
  autoSyncMinutes: 15,
  onedrive: {
    shareUrl:
      'https://ioworkspace-my.sharepoint.com/:f:/r/personal/gary_wenneker_iodigital_com/Documents/.ax',
  },
  github: { repoUrl: '', branch: 'main', subpath: '.ax', token: '' },
};

export const IMPORT_MODE_OPTIONS: { value: ShareImportMode; label: string; description: string }[] = [
  {
    value: 'review',
    label: 'Review conflicts',
    description: 'Conflicts land in .ax/policy/pending/ until approved.',
  },
  {
    value: 'merge',
    label: 'Merge',
    description: 'Apply new items; conflicts still staged as pending.',
  },
  {
    value: 'force',
    label: 'Force overwrite',
    description: 'Remote wins on hash mismatch.',
  },
];

export const AUTO_SYNC_OPTIONS = [
  { value: 0, label: 'Manual only' },
  { value: 5, label: 'Every 5 minutes' },
  { value: 15, label: 'Every 15 minutes' },
  { value: 30, label: 'Every 30 minutes' },
  { value: 60, label: 'Every hour' },
];
