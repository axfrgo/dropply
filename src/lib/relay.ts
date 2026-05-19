import { exportPairManifest, exportRelayBlob } from "./api";
import type { Item, RelayItem } from "./types";

const API_BASE = "https://dropply-backend.fortifie.com";
// The hosted relay starts rejecting JSON bodies somewhere between ~520 KB and
// ~650 KB, which browsers surface as a misleading CORS failure when the proxy
// responds before Fastify can attach CORS headers.
const MAX_RELAY_SNAPSHOT_JSON_CHARS = 480 * 1024;
const MAX_RELAY_SNAPSHOT_INLINE_B64_CHARS = 192 * 1024;
const BASE_RELAY_BLOB_CHUNK_BYTES = 128 * 1024;
const MEDIUM_RELAY_BLOB_CHUNK_BYTES = 256 * 1024;
const LARGE_RELAY_BLOB_CHUNK_BYTES = 512 * 1024;
const SMALL_RELAY_BLOB_PARALLEL_UPLOADS = 2;
const MEDIUM_RELAY_BLOB_PARALLEL_UPLOADS = 3;
const LARGE_RELAY_BLOB_PARALLEL_UPLOADS = 4;
const RETRYABLE_RELAY_STATUSES = new Set([408, 425, 429, 500, 502, 503, 504]);
const MAX_RELAY_REQUEST_RETRIES = 5;
const RELAY_RETRY_BASE_DELAY_MS = 900;
const RELAY_RETRY_MAX_DELAY_MS = 8_000;

function canInlineRelayBytes(item: RelayItem) {
  return item.type === "image" && (item.mime_type?.startsWith("image/") ?? true);
}

export type PairStatus = {
  token: string;
  devices: PairDevice[];
  paired: boolean;
  pairedDeviceCount: number;
  itemCount: number;
};

export type PairDevice = {
  deviceId: string;
  deviceType: "desktop" | "web" | "mobile";
  label: string;
  lastSeenAt: number;
  transportPreference: "relay" | "p2p";
};

export type WebRtcIceServer = {
  urls: string | string[];
  username?: string;
  credential?: string;
};

export type WebRtcSignalMessage = {
  id: string;
  fromDeviceId: string;
  toDeviceId: string;
  kind: "offer" | "answer" | "ice";
  payload: unknown;
  createdAt: number;
};

export type RelayBlobUploadProgress = {
  itemId: string;
  fileName: string;
  sentChunks: number;
  totalChunks: number;
  sentBytes: number;
  totalBytes: number;
  phase: "resuming" | "uploading" | "retrying" | "complete";
  attempt: number;
  totalAttempts: number;
};

export type RelayBlobUploadStatus = {
  ok: true;
  itemId: string;
  resumable: boolean;
  totalChunks: number;
  receivedChunks: number;
  complete: boolean;
  receivedRanges: Array<{ start: number; end: number }>;
  updated_at?: string | null;
  mime_type?: string | null;
  size_bytes?: number | null;
  sha256?: string | null;
};

function buildRelaySnapshot(input: { token: string; deviceId: string; items: RelayItem[] }) {
  let remainingInlineBudget = MAX_RELAY_SNAPSHOT_INLINE_B64_CHARS;
  const snapshot: RelayItem[] = [];

  for (const item of [...input.items].sort((left, right) =>
    left.updated_at < right.updated_at ? 1 : left.updated_at > right.updated_at ? -1 : 0
  )) {
    let candidate =
      item.bytes_b64 && !canInlineRelayBytes(item)
        ? {
            ...item,
            bytes_b64: undefined,
          }
        : item;

    if (candidate.bytes_b64) {
      if (candidate.bytes_b64.length > remainingInlineBudget) {
        candidate = {
          ...candidate,
          bytes_b64: undefined,
        };
      }
    }

    const nextSnapshot = [...snapshot, candidate];
    const nextPayloadLength = JSON.stringify({
      token: input.token,
      deviceId: input.deviceId,
      items: nextSnapshot,
    }).length;

    if (nextPayloadLength > MAX_RELAY_SNAPSHOT_JSON_CHARS) {
      if (!item.bytes_b64) {
        continue;
      }

      candidate = {
        ...item,
        bytes_b64: undefined,
      };

      const trimmedSnapshot = [...snapshot, candidate];
      const trimmedPayloadLength = JSON.stringify({
        token: input.token,
        deviceId: input.deviceId,
        items: trimmedSnapshot,
      }).length;

      if (trimmedPayloadLength > MAX_RELAY_SNAPSHOT_JSON_CHARS) {
        continue;
      }
    }

    if (candidate.bytes_b64) {
      remainingInlineBudget -= candidate.bytes_b64.length;
    }

    snapshot.push(candidate);
  }

  return snapshot;
}

async function readErrorMessage(response: Response, fallback: string) {
  try {
    const payload = (await response.json()) as { error?: string };
    return payload.error || fallback;
  } catch {
    return fallback;
  }
}

function isRetryableRelayStatus(status: number) {
  return RETRYABLE_RELAY_STATUSES.has(status);
}

function parseRetryAfterMs(response: Response) {
  const rawValue = response.headers.get("retry-after");
  if (!rawValue) {
    return null;
  }

  const asSeconds = Number(rawValue);
  if (Number.isFinite(asSeconds) && asSeconds > 0) {
    return asSeconds * 1_000;
  }

  const asDate = Date.parse(rawValue);
  if (Number.isFinite(asDate)) {
    return Math.max(0, asDate - Date.now());
  }

  return null;
}

function computeRetryDelayMs(attempt: number, retryAfterMs: number | null) {
  if (retryAfterMs && retryAfterMs > 0) {
    return Math.min(retryAfterMs, RELAY_RETRY_MAX_DELAY_MS);
  }

  const exponentialDelay = Math.min(
    RELAY_RETRY_BASE_DELAY_MS * 2 ** Math.max(0, attempt - 1),
    RELAY_RETRY_MAX_DELAY_MS
  );
  const jitter = Math.floor(Math.random() * 250);
  return exponentialDelay + jitter;
}

function wait(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function base64ToBytes(base64: string) {
  const binary = window.atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function expandReceivedRanges(ranges: Array<{ start: number; end: number }>) {
  const uploadedChunks = new Set<number>();

  for (const range of ranges) {
    for (let chunkIndex = range.start; chunkIndex <= range.end; chunkIndex += 1) {
      uploadedChunks.add(chunkIndex);
    }
  }

  return uploadedChunks;
}

function resolveRelayBlobChunkBytes(input: { sizeBytes: number; mimeType?: string | null }) {
  if ((input.mimeType?.startsWith("video/") ?? false) || input.sizeBytes >= 32 * 1024 * 1024) {
    return LARGE_RELAY_BLOB_CHUNK_BYTES;
  }

  if (input.sizeBytes >= 8 * 1024 * 1024) {
    return MEDIUM_RELAY_BLOB_CHUNK_BYTES;
  }

  return BASE_RELAY_BLOB_CHUNK_BYTES;
}

function resolveRelayBlobUploadConcurrency(input: { sizeBytes: number; mimeType?: string | null }) {
  if ((input.mimeType?.startsWith("video/") ?? false) || input.sizeBytes >= 32 * 1024 * 1024) {
    return LARGE_RELAY_BLOB_PARALLEL_UPLOADS;
  }

  if (input.sizeBytes >= 8 * 1024 * 1024) {
    return MEDIUM_RELAY_BLOB_PARALLEL_UPLOADS;
  }

  return SMALL_RELAY_BLOB_PARALLEL_UPLOADS;
}

function chunkByteLength(chunkIndex: number, totalBytes: number, chunkBytes: number) {
  const start = chunkIndex * chunkBytes;
  const end = Math.min(start + chunkBytes, totalBytes);
  return Math.max(0, end - start);
}

function uploadedByteCount(uploadedChunks: Set<number>, totalBytes: number, chunkBytes: number) {
  let sentBytes = 0;
  for (const chunkIndex of uploadedChunks) {
    sentBytes += chunkByteLength(chunkIndex, totalBytes, chunkBytes);
  }
  return sentBytes;
}

async function postRelayJsonWithRetry(input: {
  url: string;
  body: string;
  fallbackMessage: string;
  onRetry?: (details: { attempt: number; totalAttempts: number; delayMs: number; status?: number }) => void;
}) {
  let lastNetworkError: unknown = null;

  for (let attempt = 1; attempt <= MAX_RELAY_REQUEST_RETRIES; attempt += 1) {
    let response: Response;

    try {
      response = await fetch(input.url, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: input.body,
      });
    } catch (error) {
      lastNetworkError = error;
      if (attempt >= MAX_RELAY_REQUEST_RETRIES) {
        break;
      }

      const delayMs = computeRetryDelayMs(attempt, null);
      input.onRetry?.({
        attempt: attempt + 1,
        totalAttempts: MAX_RELAY_REQUEST_RETRIES,
        delayMs,
      });
      await wait(delayMs);
      continue;
    }

    if (response.ok) {
      return response;
    }

    if (attempt < MAX_RELAY_REQUEST_RETRIES && isRetryableRelayStatus(response.status)) {
      const delayMs = computeRetryDelayMs(attempt, parseRetryAfterMs(response));
      input.onRetry?.({
        attempt: attempt + 1,
        totalAttempts: MAX_RELAY_REQUEST_RETRIES,
        delayMs,
        status: response.status,
      });
      await wait(delayMs);
      continue;
    }

    throw new Error(await readErrorMessage(response, input.fallbackMessage));
  }

  if (lastNetworkError instanceof Error) {
    throw lastNetworkError;
  }

  throw new Error(input.fallbackMessage);
}

async function postRelayBinaryWithRetry(input: {
  url: string;
  body: Uint8Array;
  fallbackMessage: string;
  onRetry?: (details: { attempt: number; totalAttempts: number; delayMs: number; status?: number }) => void;
}) {
  let lastNetworkError: unknown = null;

  for (let attempt = 1; attempt <= MAX_RELAY_REQUEST_RETRIES; attempt += 1) {
    let response: Response;

    try {
      const bytes = new Uint8Array(input.body.byteLength);
      bytes.set(input.body);
      const body = new Blob([bytes.buffer], {
        type: "application/octet-stream",
      });
      response = await fetch(input.url, {
        method: "POST",
        headers: {
          "Content-Type": "application/octet-stream",
        },
        body,
      });
    } catch (error) {
      lastNetworkError = error;
      if (attempt >= MAX_RELAY_REQUEST_RETRIES) {
        break;
      }

      const delayMs = computeRetryDelayMs(attempt, null);
      input.onRetry?.({
        attempt: attempt + 1,
        totalAttempts: MAX_RELAY_REQUEST_RETRIES,
        delayMs,
      });
      await wait(delayMs);
      continue;
    }

    if (response.ok) {
      return response;
    }

    if (attempt < MAX_RELAY_REQUEST_RETRIES && isRetryableRelayStatus(response.status)) {
      const delayMs = computeRetryDelayMs(attempt, parseRetryAfterMs(response));
      input.onRetry?.({
        attempt: attempt + 1,
        totalAttempts: MAX_RELAY_REQUEST_RETRIES,
        delayMs,
        status: response.status,
      });
      await wait(delayMs);
      continue;
    }

    throw new Error(await readErrorMessage(response, input.fallbackMessage));
  }

  if (lastNetworkError instanceof Error) {
    throw lastNetworkError;
  }

  throw new Error(input.fallbackMessage);
}

export async function registerPairingDevice(input: {
  token: string;
  deviceId: string;
  deviceType: "desktop" | "web" | "mobile";
  label: string;
  transportPreference?: "relay" | "p2p";
}) {
  const response = await fetch(`${API_BASE}/v1/pair/register`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Pairing registration failed."));
  }

  return (await response.json()) as PairStatus;
}

export async function fetchPairStatus(token: string, deviceId: string) {
  const response = await fetch(
    `${API_BASE}/v1/pair/status?token=${encodeURIComponent(token)}&deviceId=${encodeURIComponent(deviceId)}`
  );
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Pair status unavailable."));
  }
  return (await response.json()) as PairStatus;
}

export async function removePairingDevice(input: {
  token: string;
  deviceId: string;
  targetDeviceId: string;
}) {
  const response = await fetch(`${API_BASE}/v1/pair/remove`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Failed to remove paired device."));
  }

  return (await response.json()) as PairStatus;
}

export async function fetchRelayPull(token: string, deviceId: string) {
  const response = await fetch(
    `${API_BASE}/v1/relay/pull?token=${encodeURIComponent(token)}&deviceId=${encodeURIComponent(deviceId)}`
  );
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Relay pull failed."));
  }
  return (await response.json()) as PairStatus & { items: RelayItem[] };
}

export async function fetchRelayItem(token: string, deviceId: string, itemId: string) {
  const response = await fetch(
    `${API_BASE}/v1/relay/item?token=${encodeURIComponent(token)}&deviceId=${encodeURIComponent(deviceId)}&itemId=${encodeURIComponent(itemId)}`
  );
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Relay item fetch failed."));
  }
  return (await response.json()) as RelayItem;
}

export async function fetchRelayBlobUploadStatus(input: {
  token: string;
  deviceId: string;
  itemId: string;
  updatedAt: string;
  totalChunks: number;
  sizeBytes?: number | null;
  sha256?: string | null;
}) {
  const params = new URLSearchParams({
    token: input.token,
    deviceId: input.deviceId,
    itemId: input.itemId,
    updated_at: input.updatedAt,
    totalChunks: String(input.totalChunks),
  });

  if (input.sizeBytes != null) {
    params.set("size_bytes", String(input.sizeBytes));
  }

  if (input.sha256) {
    params.set("sha256", input.sha256);
  }

  const response = await fetch(`${API_BASE}/v1/relay/blob/status?${params.toString()}`);
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Relay blob status unavailable."));
  }

  return (await response.json()) as RelayBlobUploadStatus;
}

export async function fetchWebRtcConfig() {
  const response = await fetch(`${API_BASE}/v1/webrtc/config`);
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "WebRTC configuration unavailable."));
  }
  return (await response.json()) as { iceServers: WebRtcIceServer[] };
}

export async function pushWebRtcSignal(input: {
  token: string;
  fromDeviceId: string;
  toDeviceId: string;
  kind: "offer" | "answer" | "ice";
  payload: unknown;
}) {
  const response = await fetch(`${API_BASE}/v1/webrtc/signal`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "Failed to queue WebRTC signal."));
  }

  return (await response.json()) as { ok: true; queued: number; updatedAt: number };
}

export async function pullWebRtcSignals(token: string, deviceId: string) {
  const response = await fetch(
    `${API_BASE}/v1/webrtc/signal/pull?token=${encodeURIComponent(token)}&deviceId=${encodeURIComponent(deviceId)}`
  );
  if (!response.ok) {
    throw new Error(await readErrorMessage(response, "WebRTC signal pull failed."));
  }

  return (await response.json()) as { ok: true; signals: WebRtcSignalMessage[]; updatedAt: number };
}

export async function pushDesktopRelaySnapshot(input: {
  token: string;
  deviceId: string;
  includeInlineMedia: boolean;
}) {
  const items = buildRelaySnapshot({
    token: input.token,
    deviceId: input.deviceId,
    items: (await exportPairManifest()).map((item) =>
      input.includeInlineMedia || item.type === "text"
        ? item
        : {
            ...item,
            bytes_b64: undefined,
          }
    ),
  });
  const response = await postRelayJsonWithRetry({
    url: `${API_BASE}/v1/relay/push`,
    body: JSON.stringify({
      token: input.token,
      deviceId: input.deviceId,
      items,
    }),
    fallbackMessage: "Relay push failed.",
  });

  return (await response.json()) as { ok: true; itemCount: number; updatedAt: number };
}

export async function pushDesktopRelayBlob(input: {
  token: string;
  deviceId: string;
  item: Item;
  onProgress?: (progress: RelayBlobUploadProgress) => void;
}) {
  if (input.item.type === "text") {
    return;
  }

  const chunkBytes = resolveRelayBlobChunkBytes({
    sizeBytes: input.item.size_bytes ?? 0,
    mimeType: input.item.mime_type,
  });
  const uploadConcurrency = resolveRelayBlobUploadConcurrency({
    sizeBytes: input.item.size_bytes ?? 0,
    mimeType: input.item.mime_type,
  });
  const blob = await exportRelayBlob(input.item.id, chunkBytes);

  if (!blob.chunks.length) {
    return;
  }

  let uploadedChunks = new Set<number>();
  let completedChunks = 0;
  let completedBytes = 0;

  try {
    const blobStatus = await fetchRelayBlobUploadStatus({
      token: input.token,
      deviceId: input.deviceId,
      itemId: input.item.id,
      updatedAt: blob.updated_at,
      totalChunks: blob.chunks.length,
      sizeBytes: blob.size_bytes,
      sha256: blob.sha256 ?? input.item.sha256 ?? null,
    });
    uploadedChunks = blobStatus.resumable ? expandReceivedRanges(blobStatus.receivedRanges) : new Set<number>();
    completedChunks = Math.min(uploadedChunks.size, blob.chunks.length);
    completedBytes = uploadedByteCount(uploadedChunks, blob.size_bytes, chunkBytes);
  } catch {
    uploadedChunks = new Set<number>();
    completedChunks = 0;
    completedBytes = 0;
  }

  if (completedChunks > 0 && completedChunks < blob.chunks.length) {
    input.onProgress?.({
      itemId: input.item.id,
      fileName: input.item.name ?? input.item.id,
      sentChunks: completedChunks,
      totalChunks: blob.chunks.length,
      sentBytes: completedBytes,
      totalBytes: blob.size_bytes,
      phase: "resuming",
      attempt: 1,
      totalAttempts: MAX_RELAY_REQUEST_RETRIES,
    });
  }

  if (completedChunks >= blob.chunks.length) {
    input.onProgress?.({
      itemId: input.item.id,
      fileName: input.item.name ?? input.item.id,
      sentChunks: blob.chunks.length,
      totalChunks: blob.chunks.length,
      sentBytes: blob.size_bytes,
      totalBytes: blob.size_bytes,
      phase: "complete",
      attempt: 1,
      totalAttempts: MAX_RELAY_REQUEST_RETRIES,
    });
    return;
  }

  const pendingChunkIndices = Array.from({ length: blob.chunks.length }, (_, chunkIndex) => chunkIndex).filter(
    (chunkIndex) => !uploadedChunks.has(chunkIndex)
  );
  let nextPendingChunkIndex = 0;

  async function uploadChunk(chunkIndex: number) {
    const params = new URLSearchParams({
      token: input.token,
      deviceId: input.deviceId,
      itemId: input.item.id,
      updated_at: blob.updated_at,
      mime_type: blob.mime_type ?? input.item.mime_type ?? "application/octet-stream",
      size_bytes: String(blob.size_bytes),
      totalChunks: String(blob.chunks.length),
      chunkIndex: String(chunkIndex),
    });
    if (blob.sha256 ?? input.item.sha256) {
      params.set("sha256", blob.sha256 ?? input.item.sha256 ?? "");
    }

    const response = await postRelayBinaryWithRetry({
      url: `${API_BASE}/v1/relay/blob/push-binary?${params.toString()}`,
      body: base64ToBytes(blob.chunks[chunkIndex]),
      fallbackMessage: `Relay blob push failed for ${input.item.name ?? input.item.id}.`,
      onRetry: ({ attempt, totalAttempts }) => {
        input.onProgress?.({
          itemId: input.item.id,
          fileName: input.item.name ?? input.item.id,
          sentChunks: completedChunks,
          totalChunks: blob.chunks.length,
          sentBytes: completedBytes,
          totalBytes: blob.size_bytes,
          phase: "retrying",
          attempt,
          totalAttempts,
        });
      },
    });
    await response.text().catch(() => "");
    completedBytes += chunkByteLength(chunkIndex, blob.size_bytes, chunkBytes);
    completedChunks += 1;
    input.onProgress?.({
      itemId: input.item.id,
      fileName: input.item.name ?? input.item.id,
      sentChunks: completedChunks,
      totalChunks: blob.chunks.length,
      sentBytes: completedBytes,
      totalBytes: blob.size_bytes,
      phase: "uploading",
      attempt: 1,
      totalAttempts: MAX_RELAY_REQUEST_RETRIES,
    });
  }

  async function worker() {
    while (nextPendingChunkIndex < pendingChunkIndices.length) {
      const queueIndex = nextPendingChunkIndex;
      nextPendingChunkIndex += 1;
      const chunkIndex = pendingChunkIndices[queueIndex];
      await uploadChunk(chunkIndex);
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(uploadConcurrency, pendingChunkIndices.length) }, () => worker())
  );

  input.onProgress?.({
    itemId: input.item.id,
    fileName: input.item.name ?? input.item.id,
    sentChunks: blob.chunks.length,
    totalChunks: blob.chunks.length,
    sentBytes: blob.size_bytes,
    totalBytes: blob.size_bytes,
    phase: "complete",
    attempt: 1,
    totalAttempts: MAX_RELAY_REQUEST_RETRIES,
  });
}
