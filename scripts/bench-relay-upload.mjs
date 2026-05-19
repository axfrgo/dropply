import { createHash, randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { basename, extname } from "node:path";

const [, , filePathArg] = process.argv;

if (!filePathArg) {
  console.error("Usage: node scripts/bench-relay-upload.mjs <file-path>");
  process.exit(1);
}

const API_BASE = process.env.DROPPLY_API_BASE_URL || "https://dropply-backend.fortifie.com";
const filePath = filePathArg;

function nowMs() {
  return Number(process.hrtime.bigint() / 1_000_000n);
}

function inferMimeType(filePathValue) {
  const ext = extname(filePathValue).toLowerCase();
  switch (ext) {
    case ".mp4":
      return "video/mp4";
    case ".mov":
      return "video/quicktime";
    case ".mkv":
      return "video/x-matroska";
    case ".webm":
      return "video/webm";
    case ".png":
      return "image/png";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    default:
      return "application/octet-stream";
  }
}

function resolveChunkBytes(sizeBytes, mimeType) {
  if (mimeType.startsWith("video/") || sizeBytes >= 32 * 1024 * 1024) {
    return 512 * 1024;
  }
  if (sizeBytes >= 8 * 1024 * 1024) {
    return 256 * 1024;
  }
  return 128 * 1024;
}

function resolveConcurrency(sizeBytes, mimeType) {
  if (mimeType.startsWith("video/") || sizeBytes >= 32 * 1024 * 1024) {
    return 4;
  }
  if (sizeBytes >= 8 * 1024 * 1024) {
    return 3;
  }
  return 2;
}

async function fetchJson(url, options = {}, label = "request") {
  const response = await fetch(url, options);
  if (!response.ok) {
    let message = `${label} failed with HTTP ${response.status}`;
    try {
      const payload = await response.json();
      if (payload?.error) {
        message = `${label} failed: ${payload.error}`;
      }
    } catch {
      // ignore
    }
    throw new Error(message);
  }
  return response.json();
}

async function postJson(url, body) {
  return fetchJson(
    url,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    },
    "POST"
  );
}

async function fetchChunk(token, deviceId, itemId, chunkIndex) {
  const response = await fetch(
    `${API_BASE}/v1/relay/blob/chunk-binary?${new URLSearchParams({
      token,
      deviceId,
      itemId,
      chunkIndex: String(chunkIndex),
    }).toString()}`
  );

  if (!response.ok) {
    let message = `chunk ${chunkIndex} failed with HTTP ${response.status}`;
    try {
      const payload = await response.json();
      if (payload?.error) {
        message = `chunk ${chunkIndex} failed: ${payload.error}`;
      }
    } catch {
      // ignore
    }
    throw new Error(message);
  }

  return Buffer.from(await response.arrayBuffer());
}

async function main() {
  const fileBytes = await readFile(filePath);
  const sizeBytes = fileBytes.byteLength;
  const mimeType = inferMimeType(filePath);
  const chunkBytes = resolveChunkBytes(sizeBytes, mimeType);
  const concurrency = resolveConcurrency(sizeBytes, mimeType);
  const totalChunks = Math.max(1, Math.ceil(sizeBytes / chunkBytes));
  const sha256 = createHash("sha256").update(fileBytes).digest("hex");

  const token = `bench-${Date.now()}-${Math.floor(Math.random() * 10_000)}`;
  const senderDeviceId = randomUUID();
  const receiverDeviceId = randomUUID();
  const itemId = randomUUID();
  const updatedAt = new Date().toISOString();

  console.log(`File: ${filePath}`);
  console.log(`Size: ${sizeBytes} bytes`);
  console.log(`SHA-256: ${sha256}`);
  console.log(`Chunk size: ${chunkBytes} bytes`);
  console.log(`Total chunks: ${totalChunks}`);
  console.log(`Parallel uploads: ${concurrency}`);
  console.log(`Token: ${token}`);

  await fetchJson(
    `${API_BASE}/v1/pair/register`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        token,
        deviceId: senderDeviceId,
        deviceType: "desktop",
        label: "Dropply bench sender",
        transportPreference: "relay",
      }),
    },
    "sender pair registration"
  );

  await fetchJson(
    `${API_BASE}/v1/pair/register`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        token,
        deviceId: receiverDeviceId,
        deviceType: "mobile",
        label: "Dropply bench receiver",
        transportPreference: "relay",
      }),
    },
    "receiver pair registration"
  );

  const uploadStartMs = nowMs();
  const statusResponse = await fetchJson(
    `${API_BASE}/v1/relay/blob/status?${new URLSearchParams({
      token,
      deviceId: senderDeviceId,
      itemId,
      updated_at: updatedAt,
      totalChunks: String(totalChunks),
      size_bytes: String(sizeBytes),
      sha256,
    }).toString()}`,
    undefined,
    "relay blob status"
  );

  const uploadedChunks = new Set();
  for (const range of statusResponse.receivedRanges ?? []) {
    for (let chunkIndex = range.start; chunkIndex <= range.end; chunkIndex += 1) {
      uploadedChunks.add(chunkIndex);
    }
  }

  const pendingChunkIndices = Array.from({ length: totalChunks }, (_, chunkIndex) => chunkIndex).filter(
    (chunkIndex) => !uploadedChunks.has(chunkIndex)
  );
  let nextIndex = 0;
  let completedChunks = uploadedChunks.size;

  process.stdout.write(`Uploading: ${completedChunks}/${totalChunks}`);

  async function worker() {
    while (nextIndex < pendingChunkIndices.length) {
      const queueIndex = nextIndex;
      nextIndex += 1;
      const chunkIndex = pendingChunkIndices[queueIndex];
      const start = chunkIndex * chunkBytes;
      const end = Math.min(start + chunkBytes, sizeBytes);
      const chunkBytesBuffer = fileBytes.subarray(start, end);

      const response = await fetch(
        `${API_BASE}/v1/relay/blob/push-binary?${new URLSearchParams({
          token,
          deviceId: senderDeviceId,
          itemId,
          updated_at: updatedAt,
          mime_type: mimeType,
          size_bytes: String(sizeBytes),
          sha256,
          totalChunks: String(totalChunks),
          chunkIndex: String(chunkIndex),
        }).toString()}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/octet-stream",
          },
          body: chunkBytesBuffer,
        }
      );

      if (!response.ok) {
        let message = `relay blob push chunk ${chunkIndex + 1}/${totalChunks} failed with HTTP ${response.status}`;
        try {
          const payload = await response.json();
          if (payload?.error) {
            message = `relay blob push chunk ${chunkIndex + 1}/${totalChunks} failed: ${payload.error}`;
          }
        } catch {
          // ignore
        }
        throw new Error(message);
      }

      completedChunks += 1;
      process.stdout.write(`\rUploading: ${completedChunks}/${totalChunks}`);
    }
  }

  await Promise.all(Array.from({ length: Math.min(concurrency, pendingChunkIndices.length || 1) }, () => worker()));
  process.stdout.write("\n");
  const uploadEndMs = nowMs();

  const itemType = mimeType.startsWith("image/") ? "image" : "file";
  const publishStartMs = nowMs();
  await postJson(`${API_BASE}/v1/relay/push`, {
    token,
    deviceId: senderDeviceId,
    items: [
      {
        id: itemId,
        type: itemType,
        name: basename(filePath),
        mime_type: mimeType,
        size_bytes: sizeBytes,
        sha256,
        updated_at: updatedAt,
        device_id: senderDeviceId,
      },
    ],
  });
  const publishEndMs = nowMs();

  const pullStartMs = nowMs();
  const pullPayload = await fetchJson(
    `${API_BASE}/v1/relay/pull?${new URLSearchParams({
      token,
      deviceId: receiverDeviceId,
    }).toString()}`,
    undefined,
    "receiver relay pull"
  );
  const receivedItem = (pullPayload.items || []).find((item) => item.id === itemId);
  if (!receivedItem) {
    throw new Error("Uploaded item did not appear in relay pull for receiver.");
  }

  const downloadedChunks = Array.from({ length: totalChunks }, () => null);
  let nextDownloadChunkIndex = 0;
  let completedDownloadChunks = 0;

  process.stdout.write(`Downloading: 0/${totalChunks}`);

  async function downloadWorker() {
    while (nextDownloadChunkIndex < totalChunks) {
      const chunkIndex = nextDownloadChunkIndex;
      nextDownloadChunkIndex += 1;
      downloadedChunks[chunkIndex] = await fetchChunk(token, receiverDeviceId, itemId, chunkIndex);
      completedDownloadChunks += 1;
      process.stdout.write(`\rDownloading: ${completedDownloadChunks}/${totalChunks}`);
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(concurrency, totalChunks) }, () => downloadWorker())
  );
  process.stdout.write("\n");

  const downloadedBytes = Buffer.concat(downloadedChunks);
  const pullEndMs = nowMs();

  const downloadedSha256 = createHash("sha256").update(downloadedBytes).digest("hex");
  const verified = downloadedSha256 === sha256 && downloadedBytes.byteLength === sizeBytes;

  console.log("");
  console.log("Benchmark result");
  console.log("----------------");
  console.log(`Upload time: ${((uploadEndMs - uploadStartMs) / 1000).toFixed(2)}s`);
  console.log(`Publish time: ${((publishEndMs - publishStartMs) / 1000).toFixed(2)}s`);
  console.log(`Receiver pull+download time: ${((pullEndMs - pullStartMs) / 1000).toFixed(2)}s`);
  console.log(`Total end-to-end time: ${((pullEndMs - uploadStartMs) / 1000).toFixed(2)}s`);
  console.log(`Verified: ${verified ? "yes" : "no"}`);
  console.log(`Downloaded SHA-256: ${downloadedSha256}`);

  if (!verified) {
    process.exitCode = 2;
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
