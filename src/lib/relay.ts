import { exportRelayItems } from "./api";

const API_BASE = "https://dropply-backend.fortifie.com";

export type PairStatus = {
  token: string;
  devices: Array<{
    deviceId: string;
    deviceType: "desktop" | "web" | "mobile";
    label: string;
    lastSeenAt: number;
  }>;
  paired: boolean;
  pairedDeviceCount: number;
  itemCount: number;
};

export async function registerPairingDevice(input: {
  token: string;
  deviceId: string;
  deviceType: "desktop" | "web" | "mobile";
  label: string;
}) {
  const response = await fetch(`${API_BASE}/v1/pair/register`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    throw new Error("Pairing registration failed.");
  }

  return (await response.json()) as PairStatus;
}

export async function fetchPairStatus(token: string, deviceId: string) {
  const response = await fetch(
    `${API_BASE}/v1/pair/status?token=${encodeURIComponent(token)}&deviceId=${encodeURIComponent(deviceId)}`
  );
  if (!response.ok) {
    throw new Error("Pair status unavailable.");
  }
  return (await response.json()) as PairStatus;
}

export async function pushDesktopRelaySnapshot(input: {
  token: string;
  deviceId: string;
}) {
  const items = await exportRelayItems();
  const response = await fetch(`${API_BASE}/v1/relay/push`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      token: input.token,
      deviceId: input.deviceId,
      items,
    }),
  });

  if (!response.ok) {
    throw new Error("Relay push failed.");
  }

  return (await response.json()) as { ok: true; itemCount: number; updatedAt: number };
}
