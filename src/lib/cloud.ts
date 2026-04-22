export type CloudProvider = {
  id: "google" | "magic_link" | "passkey";
  label: string;
  enabled: boolean;
};

export type CloudConfig = {
  app_name: string;
  public_web_url: string;
  auth_provider: string;
  auth_methods: string[];
  local_mode_requires_login: boolean;
  hosted_sync_requires_login: boolean;
};

const CLOUD_API_BASE = "https://dropply-backend.fortifie.com";

export async function fetchCloudHealth(): Promise<boolean> {
  const response = await fetch(`${CLOUD_API_BASE}/health`);
  return response.ok;
}

export async function fetchCloudConfig(): Promise<CloudConfig> {
  const response = await fetch(`${CLOUD_API_BASE}/v1/public/config`);
  if (!response.ok) {
    throw new Error("Cloud config unavailable.");
  }
  return response.json();
}

export async function fetchCloudProviders(): Promise<CloudProvider[]> {
  const response = await fetch(`${CLOUD_API_BASE}/v1/auth/providers`);
  if (!response.ok) {
    throw new Error("Auth providers unavailable.");
  }
  const payload = (await response.json()) as { providers: CloudProvider[] };
  return payload.providers;
}

export async function openHostedAuthPath(path: "/signin" | "/signup"): Promise<string> {
  const config = await fetchCloudConfig();
  return `${config.public_web_url}${path}`;
}
