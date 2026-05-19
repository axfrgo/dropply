import { BaseDirectory, open, mkdir, remove, writeFile } from "@tauri-apps/plugin-fs";
import { appLocalDataDir, join } from "@tauri-apps/api/path";
import { exportPairManifest, importStagedTransfer } from "./api";
import {
  fetchWebRtcConfig,
  pullWebRtcSignals,
  pushWebRtcSignal,
  type WebRtcIceServer,
  type WebRtcSignalMessage,
} from "./relay";
import type { Item, RelayItem } from "./types";

const CHUNK_SIZE = 128 * 1024;
const CHUNK_HEADER_BYTES = 40;
const BUFFERED_AMOUNT_HIGH_WATER = 4 * 1024 * 1024;
const SIGNAL_POLL_MS = 2000;
const STAGING_DIR = "incoming-transfers";
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

type PeerContext = {
  peer: RTCPeerConnection;
  channel: RTCDataChannel | null;
  remoteDeviceId: string;
  incomingTransfers: Map<string, IncomingTransfer>;
  pendingIceCandidates: RTCIceCandidateInit[];
  closing: boolean;
};

type IncomingTransfer = {
  transferId: string;
  item: RelayItem;
  relativePath: string;
  absolutePath: string;
  totalChunks: number;
  nextChunkIndex: number;
};

type ControlMessage =
  | {
      kind: "manifest";
      items: RelayItem[];
    }
  | {
      kind: "download-request";
      transferId: string;
      itemId: string;
    }
  | {
      kind: "transfer-start";
      transferId: string;
      direction: "upload" | "download";
      item: RelayItem;
      totalChunks: number;
    }
  | {
      kind: "transfer-complete";
      transferId: string;
    }
  | {
      kind: "transfer-error";
      transferId: string;
      message: string;
    };

type DesktopDirectTransferOptions = {
  token: string;
  deviceId: string;
  getItemById: (itemId: string) => Item | undefined;
  onItemImported: (item: Item) => void | Promise<void>;
  onConnectionChange: (connected: boolean) => void;
  onError: (message: string) => void;
};

export type DesktopDirectTransferHandle = {
  close: () => void;
  broadcastManifest: () => Promise<void>;
  hasConnections: () => boolean;
  connectToPeer: (remoteDeviceId: string) => Promise<void>;
  disconnectPeer: (remoteDeviceId: string) => void;
  prunePeers: (allowedRemoteDeviceIds: string[]) => void;
  setSignalPolling: (active: boolean) => void;
};

export function createDesktopDirectTransfer(
  options: DesktopDirectTransferOptions
): DesktopDirectTransferHandle {
  const peers = new Map<string, PeerContext>();
  let closed = false;
  let signalTimer: number | null = null;
  let signalPollingActive = false;
  let iceServersPromise: Promise<WebRtcIceServer[]> | null = null;
  let stagingRootPromise: Promise<string> | null = null;

  function resolveStagingRoot() {
    if (!stagingRootPromise) {
      stagingRootPromise = (async () => {
        await mkdir(STAGING_DIR, { baseDir: BaseDirectory.AppLocalData, recursive: true });
        const root = await appLocalDataDir();
        return join(root, STAGING_DIR);
      })();
    }
    return stagingRootPromise;
  }

  async function resolveIceServers() {
    if (!iceServersPromise) {
      iceServersPromise = fetchWebRtcConfig()
        .then((result) => result.iceServers)
        .catch((error) => {
          options.onError(error instanceof Error ? error.message : "WebRTC signaling unavailable.");
          return [{ urls: ["stun:stun.l.google.com:19302"] }];
        });
    }
    return iceServersPromise;
  }

  function openedChannels() {
    return Array.from(peers.values()).filter((entry) => entry.channel?.readyState === "open");
  }

  function peerIsUsable(context: PeerContext) {
    return (
      !context.closing &&
      context.peer.connectionState !== "failed" &&
      context.peer.connectionState !== "closed" &&
      context.peer.connectionState !== "disconnected"
    );
  }

  function updateConnectionState() {
    options.onConnectionChange(openedChannels().length > 0);
  }

  function disconnectContext(context: PeerContext) {
    context.closing = true;
    context.channel?.close();
    context.peer.close();
    for (const transferId of context.incomingTransfers.keys()) {
      void cleanupIncomingTransfer(context, transferId);
    }
  }

  function sendControl(channel: RTCDataChannel, message: ControlMessage) {
    if (channel.readyState !== "open") {
      return;
    }
    channel.send(JSON.stringify(message));
  }

  function encodeChunkFrame(transferId: string, chunkIndex: number, chunk: Uint8Array) {
    const frame = new Uint8Array(CHUNK_HEADER_BYTES + chunk.byteLength);
    frame.set(textEncoder.encode(transferId.slice(0, 36)), 0);
    new DataView(frame.buffer).setUint32(36, chunkIndex);
    frame.set(chunk, CHUNK_HEADER_BYTES);
    return frame.buffer;
  }

  function decodeChunkFrame(buffer: ArrayBuffer) {
    const bytes = new Uint8Array(buffer);
    const transferId = textDecoder.decode(bytes.subarray(0, 36));
    const chunkIndex = new DataView(buffer).getUint32(36);
    return {
      transferId,
      chunkIndex,
      payload: bytes.subarray(CHUNK_HEADER_BYTES),
    };
  }

  async function waitForChannelDrain(channel: RTCDataChannel) {
    if (channel.bufferedAmount < BUFFERED_AMOUNT_HIGH_WATER) {
      return;
    }

    await new Promise<void>((resolve) => {
      const handleLowBuffer = () => {
        channel.removeEventListener("bufferedamountlow", handleLowBuffer);
        resolve();
      };

      channel.addEventListener("bufferedamountlow", handleLowBuffer);
    });
  }

  async function cleanupIncomingTransfer(context: PeerContext, transferId: string) {
    const incoming = context.incomingTransfers.get(transferId);
    if (!incoming) {
      return;
    }

    context.incomingTransfers.delete(transferId);
    try {
      await remove(incoming.relativePath, { baseDir: BaseDirectory.AppLocalData });
    } catch {
      // Best-effort cleanup for interrupted transfers.
    }
  }

  async function sendManifest(channel?: RTCDataChannel) {
    const manifest = await exportPairManifest();
    if (channel) {
      sendControl(channel, { kind: "manifest", items: manifest });
      return;
    }

    for (const entry of openedChannels()) {
      sendControl(entry.channel!, { kind: "manifest", items: manifest });
    }
  }

  async function sendFile(context: PeerContext, transferId: string, itemId: string) {
    const channel = context.channel;
    if (!channel || channel.readyState !== "open") {
      return;
    }

    const item = options.getItemById(itemId);
    if (!item?.storage_path) {
      sendControl(channel, {
        kind: "transfer-error",
        transferId,
        message: "The requested file is no longer available on this desktop.",
      });
      return;
    }

    try {
      const file = await open(item.storage_path, {
        baseDir: BaseDirectory.AppLocalData,
        read: true,
      });
      const fileInfo = await file.stat();
      const totalChunks = Math.max(1, Math.ceil(fileInfo.size / CHUNK_SIZE));
      const manifest = await exportPairManifest();
      const relayItem = manifest.find((entry) => entry.id === itemId);
      if (!relayItem) {
        await file.close();
        sendControl(channel, {
          kind: "transfer-error",
          transferId,
          message: "The requested file is missing from the local manifest.",
        });
        return;
      }

      sendControl(channel, {
        kind: "transfer-start",
        transferId,
        direction: "download",
        item: relayItem,
        totalChunks,
      });

      channel.bufferedAmountLowThreshold = BUFFERED_AMOUNT_HIGH_WATER / 2;
      let chunkIndex = 0;

      while (true) {
        const buffer = new Uint8Array(CHUNK_SIZE);
        const bytesRead = await file.read(buffer);
        if (bytesRead === null || bytesRead === 0) {
          break;
        }

        const payload = buffer.subarray(0, bytesRead);
        channel.send(encodeChunkFrame(transferId, chunkIndex, payload));
        chunkIndex += 1;
        await waitForChannelDrain(channel);
      }

      await file.close();
      sendControl(channel, {
        kind: "transfer-complete",
        transferId,
      });
    } catch (error) {
      sendControl(channel, {
        kind: "transfer-error",
        transferId,
        message: error instanceof Error ? error.message : "Direct file transfer failed.",
      });
    }
  }

  async function handleIncomingControl(context: PeerContext, message: ControlMessage) {
    switch (message.kind) {
      case "download-request":
        await sendFile(context, message.transferId, message.itemId);
        return;
      case "transfer-start":
        if (message.direction !== "upload") {
          return;
        }

        try {
          const stagingRoot = await resolveStagingRoot();
          const relativePath = `${STAGING_DIR}/${message.transferId}.part`;
          const absolutePath = await join(stagingRoot, `${message.transferId}.part`);
          await writeFile(relativePath, new Uint8Array(0), {
            baseDir: BaseDirectory.AppLocalData,
          });
          context.incomingTransfers.set(message.transferId, {
            transferId: message.transferId,
            item: message.item,
            relativePath,
            absolutePath,
            totalChunks: message.totalChunks,
            nextChunkIndex: 0,
          });
        } catch (error) {
          sendControl(context.channel!, {
            kind: "transfer-error",
            transferId: message.transferId,
            message: error instanceof Error ? error.message : "Unable to prepare the incoming transfer.",
          });
        }
        return;
      case "transfer-complete": {
        const incoming = context.incomingTransfers.get(message.transferId);
        if (!incoming) {
          return;
        }

        try {
          const imported = await importStagedTransfer(incoming.item, incoming.absolutePath);
          context.incomingTransfers.delete(message.transferId);
          await options.onItemImported(imported);
          await sendManifest();
        } catch (error) {
          await cleanupIncomingTransfer(context, message.transferId);
          sendControl(context.channel!, {
            kind: "transfer-error",
            transferId: message.transferId,
            message: error instanceof Error ? error.message : "Unable to import the incoming transfer.",
          });
        }
        return;
      }
      case "transfer-error":
        await cleanupIncomingTransfer(context, message.transferId);
        options.onError(message.message);
        return;
      case "manifest":
        return;
    }
  }

  async function handleIncomingBinary(context: PeerContext, payload: ArrayBuffer) {
    const frame = decodeChunkFrame(payload);
    const incoming = context.incomingTransfers.get(frame.transferId);
    if (!incoming) {
      return;
    }

    if (frame.chunkIndex !== incoming.nextChunkIndex) {
      sendControl(context.channel!, {
        kind: "transfer-error",
        transferId: frame.transferId,
        message: "The incoming transfer arrived out of order and was cancelled.",
      });
      await cleanupIncomingTransfer(context, frame.transferId);
      return;
    }

    await writeFile(incoming.relativePath, frame.payload, {
      baseDir: BaseDirectory.AppLocalData,
      append: true,
    });
    incoming.nextChunkIndex += 1;
  }

  function attachChannel(context: PeerContext, channel: RTCDataChannel) {
    context.channel = channel;
    channel.binaryType = "arraybuffer";

    channel.addEventListener("open", () => {
      updateConnectionState();
      void sendManifest(channel).catch((error) => {
        options.onError(error instanceof Error ? error.message : "Unable to send the latest media manifest.");
      });
    });

    channel.addEventListener("close", () => {
      context.channel = null;
      updateConnectionState();
    });

    channel.addEventListener("error", () => {
      if (
        closed ||
        context.closing ||
        channel.readyState === "closing" ||
        channel.readyState === "closed"
      ) {
        return;
      }

      options.onError("The direct media link disconnected. Relay is still available if needed.");
    });

    channel.addEventListener("message", (event) => {
      if (typeof event.data === "string") {
        let parsed: ControlMessage;
        try {
          parsed = JSON.parse(event.data) as ControlMessage;
        } catch {
          return;
        }
        void handleIncomingControl(context, parsed).catch((error) => {
          options.onError(error instanceof Error ? error.message : "Direct media control handling failed.");
        });
        return;
      }

      if (event.data instanceof ArrayBuffer) {
        void handleIncomingBinary(context, event.data).catch((error) => {
          options.onError(error instanceof Error ? error.message : "Incoming direct media chunk failed.");
        });
      }
    });
  }

  async function createPeer(remoteDeviceId: string) {
    const peer = new RTCPeerConnection({
      iceServers: (await resolveIceServers()) as RTCIceServer[],
    });

    const context: PeerContext = {
      peer,
      channel: null,
      remoteDeviceId,
      incomingTransfers: new Map(),
      pendingIceCandidates: [],
      closing: false,
    };

    peer.addEventListener("icecandidate", (event) => {
      if (!event.candidate) {
        return;
      }

      void pushWebRtcSignal({
        token: options.token,
        fromDeviceId: options.deviceId,
        toDeviceId: remoteDeviceId,
        kind: "ice",
        payload: event.candidate.toJSON(),
      }).catch((error) => {
        options.onError(error instanceof Error ? error.message : "Unable to send ICE candidate.");
      });
    });

    peer.addEventListener("connectionstatechange", () => {
      if (peer.connectionState === "failed" || peer.connectionState === "closed") {
        peers.delete(remoteDeviceId);
        updateConnectionState();
      }
    });

    peer.addEventListener("datachannel", (event) => {
      attachChannel(context, event.channel);
    });

    peers.set(remoteDeviceId, context);
    return context;
  }

  async function handleSignal(message: WebRtcSignalMessage) {
    let context = peers.get(message.fromDeviceId) ?? null;

    if (message.kind === "offer") {
      if (context) {
        disconnectContext(context);
        peers.delete(message.fromDeviceId);
      }

      context = await createPeer(message.fromDeviceId);
      await context.peer.setRemoteDescription(message.payload as RTCSessionDescriptionInit);
      for (const candidate of context.pendingIceCandidates) {
        await context.peer.addIceCandidate(candidate);
      }
      context.pendingIceCandidates = [];
      const answer = await context.peer.createAnswer();
      await context.peer.setLocalDescription(answer);

      await pushWebRtcSignal({
        token: options.token,
        fromDeviceId: options.deviceId,
        toDeviceId: message.fromDeviceId,
        kind: "answer",
        payload: context.peer.localDescription,
      });
      return;
    }

    if (!context) {
      return;
    }

    if (message.kind === "answer") {
      await context.peer.setRemoteDescription(message.payload as RTCSessionDescriptionInit);
      for (const candidate of context.pendingIceCandidates) {
        await context.peer.addIceCandidate(candidate);
      }
      context.pendingIceCandidates = [];
      return;
    }

    if (message.kind === "ice" && message.payload) {
      const candidate = message.payload as RTCIceCandidateInit;
      if (!context.peer.remoteDescription) {
        context.pendingIceCandidates.push(candidate);
        return;
      }
      await context.peer.addIceCandidate(candidate);
    }
  }

  async function pollSignals() {
    if (closed) {
      return;
    }

    try {
      const result = await pullWebRtcSignals(options.token, options.deviceId);
      for (const signal of result.signals) {
        await handleSignal(signal);
      }
    } catch (error) {
      options.onError(error instanceof Error ? error.message : "Unable to read direct-transfer signals.");
    }
  }

  function stopSignalPolling() {
    if (signalTimer !== null) {
      window.clearTimeout(signalTimer);
      signalTimer = null;
    }
  }

  async function signalPollLoop() {
    if (closed || !signalPollingActive) {
      return;
    }

    await pollSignals();

    if (!closed && signalPollingActive) {
      signalTimer = window.setTimeout(() => {
        void signalPollLoop();
      }, SIGNAL_POLL_MS);
    }
  }

  function setSignalPolling(active: boolean) {
    if (closed || signalPollingActive === active) {
      return;
    }

    signalPollingActive = active;
    stopSignalPolling();

    if (active) {
      void signalPollLoop();
    }
  }

  return {
    close() {
      closed = true;
      signalPollingActive = false;
      stopSignalPolling();

      for (const context of peers.values()) {
        disconnectContext(context);
      }

      peers.clear();
      updateConnectionState();
    },
    async broadcastManifest() {
      await sendManifest();
    },
    hasConnections() {
      return openedChannels().length > 0;
    },
    disconnectPeer(remoteDeviceId: string) {
      const context = peers.get(remoteDeviceId);
      if (!context) {
        return;
      }

      disconnectContext(context);
      peers.delete(remoteDeviceId);
      updateConnectionState();
    },
    prunePeers(allowedRemoteDeviceIds: string[]) {
      const allowed = new Set(allowedRemoteDeviceIds);

      for (const [remoteDeviceId, context] of peers.entries()) {
        if (allowed.has(remoteDeviceId)) {
          continue;
        }

        disconnectContext(context);
        peers.delete(remoteDeviceId);
      }

      updateConnectionState();
    },
    setSignalPolling,
    async connectToPeer(remoteDeviceId: string) {
      const existing = peers.get(remoteDeviceId);
      if (existing && peerIsUsable(existing)) {
        return;
      }

      if (existing) {
        disconnectContext(existing);
        peers.delete(remoteDeviceId);
      }

      const context = await createPeer(remoteDeviceId);
      const dataChannel = context.peer.createDataChannel("dropply-media", { ordered: true });
      attachChannel(context, dataChannel);

      const offer = await context.peer.createOffer();
      await context.peer.setLocalDescription(offer);
      await pushWebRtcSignal({
        token: options.token,
        fromDeviceId: options.deviceId,
        toDeviceId: remoteDeviceId,
        kind: "offer",
        payload: context.peer.localDescription,
      });
    },
  };
}
