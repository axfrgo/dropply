import { useEffect, useMemo, useState, useRef } from "react";
import {
  bootstrapApp,
  copyItemText,
  deleteItem,
  exportItemToDownloads,
  importConversationBundle,
  importPaths,
  importText,
  openItem,
  updateItemIntentState,
} from "../lib/api";
import type { BootstrapPayload, CreateConversationBundleInput, IntentState, Item, SourceKind, SyncStatus } from "../lib/types";
import {
  fetchPairStatus,
  fetchRelayItem,
  fetchRelayPull,
  removePairingDevice,
  pushDesktopRelayBlob,
  pushDesktopRelaySnapshot,
  registerPairingDevice,
  type PairDevice,
  type RelayBlobUploadProgress,
} from "../lib/relay";
import { confirmAction } from "../lib/confirm";
import { createDesktopDirectTransfer, type DesktopDirectTransferHandle } from "../lib/directTransfer";
import { useI18n } from "../lib/i18n";
import { networkModeStore, useNetworkModeStore } from "../lib/preferences";

const EMPTY_STATUS: SyncStatus = {
  device_id: "booting",
  paired_devices: 0,
  transport: "offline",
  relay_connected: false,
  pending_entries: 0,
  pairing_token: "",
};

export type RelayTransferVisual = {
  itemId: string;
  label: string;
  message: string;
  phase: RelayBlobUploadProgress["phase"];
  percent: number;
  sentChunks: number;
  totalChunks: number;
  sentBytes: number;
  totalBytes: number;
  attempt: number;
  totalAttempts: number;
};

function resolveTransportStatus(
  networkMode: "relay" | "p2p",
  pairedDevices: number,
  directConnected: boolean
): SyncStatus["transport"] {
  if (networkMode === "p2p") {
    return directConnected ? "direct" : "offline";
  }

  return pairedDevices > 0 ? "relay" : "offline";
}

export function useDropply() {
  const { t } = useI18n();
  const [items, setItems] = useState<Item[]>([]);
  const [syncStatus, setSyncStatus] = useState<SyncStatus>(EMPTY_STATUS);
  const [pairedDevices, setPairedDevices] = useState<PairDevice[]>([]);
  const directTransferRef = useRef<DesktopDirectTransferHandle | null>(null);
  const directConnectedRef = useRef(false);
  const itemsRef = useRef<Item[]>([]);
  const lastRelayPushSignatureRef = useRef<string | null>(null);
  const lastRelayBlobSignaturesRef = useRef<Record<string, string>>({});
  const pairRegistrationRef = useRef<string | null>(null);
  const relayUploadClearTimerRef = useRef<number | null>(null);
  const [isHydrating, setIsHydrating] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [relayUploadProgress, setRelayUploadProgress] = useState<RelayTransferVisual | null>(null);
  const networkMode = useNetworkModeStore();

  function updateItems(updater: (current: Item[]) => Item[]) {
    setItems((current) => {
      const next = updater(current);
      itemsRef.current = next;
      return next;
    });
  }

  useEffect(() => {
    let isMounted = true;

    bootstrapApp()
      .then((payload: BootstrapPayload) => {
        if (!isMounted) {
          return;
        }
        itemsRef.current = payload.items;
        setItems(payload.items);
        setSyncStatus({
          ...payload.sync_status,
          pending_entries: payload.items.length,
          paired_devices: 0,
          transport: "offline",
        });
      })
      .catch((err: unknown) => {
        if (isMounted) {
          setError(err instanceof Error ? err.message : t("failedToBoot"));
        }
      })
      .finally(() => {
        if (isMounted) {
          setIsHydrating(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  useEffect(() => {
    directTransferRef.current?.setSignalPolling(networkMode === "p2p" && syncStatus.paired_devices > 0);
  }, [networkMode, syncStatus.paired_devices]);

  useEffect(() => {
    if (networkMode !== "relay") {
      return;
    }

    directConnectedRef.current = false;
    setSyncStatus((current) => ({
      ...current,
      transport: current.paired_devices > 0 ? "relay" : "offline",
    }));
  }, [networkMode]);

  useEffect(() => {
    if (!syncStatus.pairing_token || !syncStatus.device_id || syncStatus.device_id === "booting") {
      return;
    }

    let cancelled = false;
    let timer: number | null = null;
    const registrationKey = `${syncStatus.pairing_token}:${syncStatus.device_id}:${networkMode}`;

    async function readPairStatus() {
      if (pairRegistrationRef.current === registrationKey) {
        const status = await fetchPairStatus(syncStatus.pairing_token, syncStatus.device_id);
        const selfRegistered = status.devices.some((device) => device.deviceId === syncStatus.device_id);
        if (selfRegistered) {
          return status;
        }
      }

      const registration = await registerPairingDevice({
        token: syncStatus.pairing_token,
        deviceId: syncStatus.device_id,
        deviceType: "desktop",
        label: "Dropply desktop",
        transportPreference: networkMode,
      });
      pairRegistrationRef.current = registrationKey;
      return registration;
    }

    async function syncPairState() {
      try {
        const registration = await readPairStatus();

        if (cancelled) {
          return;
        }

        setPairedDevices(registration.devices);
        const linkedDevices = Math.max(0, registration.pairedDeviceCount - 1);
        const remoteDeviceIds = registration.devices
          .filter((device) => device.deviceId !== syncStatus.device_id)
          .map((device) => device.deviceId);

        directTransferRef.current?.prunePeers(remoteDeviceIds);
        directTransferRef.current?.setSignalPolling(networkMode === "p2p" && linkedDevices > 0);
        setSyncStatus((current) => ({
          ...current,
          paired_devices: linkedDevices,
          transport: resolveTransportStatus(networkMode, linkedDevices, directConnectedRef.current),
        }));

        if (linkedDevices > 0) {
          const remoteDesktopIds = registration.devices
            .filter(
              (device) =>
                device.deviceType === "desktop" &&
                device.deviceId !== syncStatus.device_id &&
                syncStatus.device_id.localeCompare(device.deviceId) < 0
            )
            .map((device) => device.deviceId);

          if (networkMode === "p2p") {
            for (const remoteDeviceId of remoteDesktopIds) {
              directTransferRef.current?.connectToPeer(remoteDeviceId).catch((err: unknown) => {
                setError(
                  err instanceof Error ? err.message : t("directDesktopLinkUnavailable")
                );
              });
            }
          } else {
            for (const remoteDeviceId of remoteDesktopIds) {
              directTransferRef.current?.disconnectPeer(remoteDeviceId);
            }
          }

          const remotePayload = await fetchRelayPull(syncStatus.pairing_token, syncStatus.device_id);
          const remoteItems = remotePayload.items;
          const currentItems = itemsRef.current;
          if (remoteItems && remoteItems.length > 0) {
            // Reconcile deletions
            for (const item of remoteItems) {
              if (item.deleted) {
                const exists = currentItems.some((i) => i.id === item.id);
                if (exists) {
                  await removeItem(item.id);
                }
              } else {
                // Handle new items from mobile (or other desktop)
                const exists = currentItems.some((i) => i.id === item.id);
                if (!exists) {
                  try {
                    if (networkMode === "p2p" && (item.type === "image" || item.type === "file")) {
                      continue;
                    }

                    const { importRelayItem } = await import("../lib/api");
                    const relayItem =
                      networkMode === "relay" && (item.type === "image" || item.type === "file")
                        ? await fetchRelayItem(
                            syncStatus.pairing_token,
                            syncStatus.device_id,
                            item.id
                          )
                        : item;
                    const imported = await importRelayItem(relayItem);
                    updateItems((current) => {
                      if (current.some((i) => i.id === imported.id)) return current;
                      return [imported, ...current].sort(
                        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
                      );
                    });
                  } catch (e) {
                    console.error("Failed to import relay item:", e);
                  }
                }
              }
            }
          }
        }
      } catch {
        if (!cancelled) {
          directTransferRef.current?.setSignalPolling(false);
          setSyncStatus((current) => ({
            ...current,
            paired_devices: 0,
            transport: resolveTransportStatus(networkMode, 0, directConnectedRef.current),
          }));
        }
      }
    }

    const pollLoop = async () => {
      await syncPairState();
      if (!cancelled) {
        timer = window.setTimeout(() => {
          void pollLoop();
        }, 8000);
      }
    };

    void pollLoop();

    return () => {
      cancelled = true;
      if (timer !== null) {
        window.clearTimeout(timer);
      }
    };
  }, [networkMode, syncStatus.device_id, syncStatus.pairing_token]);

  const sortedItems = useMemo(
    () =>
      [...items].sort((a, b) =>
        a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : 0,
      ),
    [items],
  );

  const relayPushSignature = useMemo(
    () =>
      items
        .map((item) => `${item.id}:${item.updated_at}`)
        .sort()
        .join("|"),
    [items],
  );

  const relayBlobPushSignature = useMemo(
    () =>
      items
        .filter((item) => item.type === "image" || item.type === "file")
        .map(
          (item) =>
            `${item.id}:${item.updated_at}:${item.sha256 ?? ""}:${item.size_bytes ?? ""}:${item.storage_path ?? ""}`
        )
        .sort()
        .join("|"),
    [items],
  );

  function relayBlobSignature(item: Item) {
    return `${item.updated_at}:${item.sha256 ?? ""}:${item.size_bytes ?? ""}`;
  }

  function clearRelayUploadStatusSoon() {
    if (relayUploadClearTimerRef.current !== null) {
      window.clearTimeout(relayUploadClearTimerRef.current);
    }

    relayUploadClearTimerRef.current = window.setTimeout(() => {
      relayUploadClearTimerRef.current = null;
      setRelayUploadProgress(null);
    }, 3_000);
  }

  function updateRelayUploadStatus(progress: RelayBlobUploadProgress) {
    const percent =
      progress.totalBytes > 0 ? Math.min(100, Math.round((progress.sentBytes / progress.totalBytes) * 100)) : 100;

    if (progress.phase === "resuming") {
      setRelayUploadProgress({
        itemId: progress.itemId,
        label: progress.fileName,
        message: t("relayUploadResuming", {
          label: progress.fileName,
          current: progress.sentChunks,
          total: progress.totalChunks,
        }),
        phase: progress.phase,
        percent,
        sentChunks: progress.sentChunks,
        totalChunks: progress.totalChunks,
        sentBytes: progress.sentBytes,
        totalBytes: progress.totalBytes,
        attempt: progress.attempt,
        totalAttempts: progress.totalAttempts,
      });
      return;
    }

    if (progress.phase === "retrying") {
      setRelayUploadProgress({
        itemId: progress.itemId,
        label: progress.fileName,
        message: t("relayUploadRetrying", {
          label: progress.fileName,
          attempt: progress.attempt,
          totalAttempts: progress.totalAttempts,
        }),
        phase: progress.phase,
        percent,
        sentChunks: progress.sentChunks,
        totalChunks: progress.totalChunks,
        sentBytes: progress.sentBytes,
        totalBytes: progress.totalBytes,
        attempt: progress.attempt,
        totalAttempts: progress.totalAttempts,
      });
      return;
    }

    if (progress.phase === "complete") {
      setRelayUploadProgress({
        itemId: progress.itemId,
        label: progress.fileName,
        message: t("relayUploadComplete", {
          label: progress.fileName,
        }),
        phase: progress.phase,
        percent: 100,
        sentChunks: progress.sentChunks,
        totalChunks: progress.totalChunks,
        sentBytes: progress.sentBytes,
        totalBytes: progress.totalBytes,
        attempt: progress.attempt,
        totalAttempts: progress.totalAttempts,
      });
      clearRelayUploadStatusSoon();
      return;
    }

    setRelayUploadProgress({
      itemId: progress.itemId,
      label: progress.fileName,
      message: t("relayUploadProgress", {
        label: progress.fileName,
        current: progress.sentChunks,
        total: progress.totalChunks,
        percent,
      }),
      phase: progress.phase,
      percent,
      sentChunks: progress.sentChunks,
      totalChunks: progress.totalChunks,
      sentBytes: progress.sentBytes,
      totalBytes: progress.totalBytes,
      attempt: progress.attempt,
      totalAttempts: progress.totalAttempts,
    });
  }

  useEffect(() => {
    return () => {
      if (relayUploadClearTimerRef.current !== null) {
        window.clearTimeout(relayUploadClearTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (isHydrating) {
      return;
    }

    if (!syncStatus.pairing_token || !syncStatus.device_id || syncStatus.device_id === "booting") {
      return;
    }

    if (syncStatus.paired_devices < 1) {
      lastRelayPushSignatureRef.current = null;
      lastRelayBlobSignaturesRef.current = {};
      setRelayUploadProgress(null);
      return;
    }

    let cancelled = false;

    async function pushLocalState() {
      let stage: "blob" | "snapshot" = networkMode === "relay" ? "blob" : "snapshot";

      try {
        const mediaItems = itemsRef.current.filter(
          (item) => item.type === "image" || item.type === "file"
        );
        const activeIds = new Set(mediaItems.map((item) => item.id));
        for (const itemId of Object.keys(lastRelayBlobSignaturesRef.current)) {
          if (!activeIds.has(itemId)) {
            delete lastRelayBlobSignaturesRef.current[itemId];
          }
        }

        if (networkMode === "relay") {
          for (const item of mediaItems) {
            const nextSignature = relayBlobSignature(item);
            if (lastRelayBlobSignaturesRef.current[item.id] === nextSignature) {
              continue;
            }

            await pushDesktopRelayBlob({
              token: syncStatus.pairing_token,
              deviceId: syncStatus.device_id,
              item,
              onProgress: updateRelayUploadStatus,
            });

            if (!cancelled) {
              lastRelayBlobSignaturesRef.current[item.id] = nextSignature;
            }
          }
        } else {
          lastRelayBlobSignaturesRef.current = {};
          setRelayUploadProgress(null);
        }

        if (cancelled || lastRelayPushSignatureRef.current === relayPushSignature) {
          return;
        }

        stage = "snapshot";
        await pushDesktopRelaySnapshot({
          token: syncStatus.pairing_token,
          deviceId: syncStatus.device_id,
          includeInlineMedia: networkMode === "relay",
        });

        if (!cancelled) {
          lastRelayPushSignatureRef.current = relayPushSignature;
        }
      } catch (err: unknown) {
        if (!cancelled) {
          setError(
            err instanceof Error
              ? err.message
              : stage === "blob"
                ? t("relayBlobUploadFailed")
                : t("relayPushFailed")
          );
        }
      }
    }

    void pushLocalState();

    return () => {
      cancelled = true;
    };
  }, [
    networkMode,
    relayPushSignature,
    relayBlobPushSignature,
    syncStatus.device_id,
    syncStatus.paired_devices,
    syncStatus.pairing_token,
    isHydrating,
  ]);

  useEffect(() => {
    if (
      !syncStatus.pairing_token ||
      !syncStatus.device_id ||
      syncStatus.device_id === "booting" ||
      networkMode !== "p2p"
    ) {
      directTransferRef.current?.close();
      directTransferRef.current = null;
      directConnectedRef.current = false;
      return;
    }

    const handle = createDesktopDirectTransfer({
      token: syncStatus.pairing_token,
      deviceId: syncStatus.device_id,
      getItemById: (itemId) => itemsRef.current.find((item) => item.id === itemId),
      onItemImported: (item) => {
        updateItems((current) => {
          const next = [item, ...current.filter((entry) => entry.id !== item.id)];
          setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
          return next;
        });
      },
      onConnectionChange: (connected) => {
        directConnectedRef.current = connected;
        setSyncStatus((current) => ({
          ...current,
          transport: resolveTransportStatus(networkMode, current.paired_devices, connected),
        }));
      },
      onError: (message) => {
        setError(message);
      },
    });
    directTransferRef.current = handle;

    return () => {
      if (directTransferRef.current === handle) {
        directTransferRef.current = null;
      }
      directConnectedRef.current = false;
      handle.close();
    };
  }, [networkMode, syncStatus.device_id, syncStatus.pairing_token]);

  useEffect(() => {
    if (networkMode !== "p2p") {
      return;
    }

    if (!directTransferRef.current?.hasConnections()) {
      return;
    }

    directTransferRef.current.broadcastManifest().catch((err: unknown) => {
      setError(err instanceof Error ? err.message : t("directManifestRefreshFailed"));
    });
  }, [networkMode, relayPushSignature, t]);

  async function addText(text: string, id?: string, sourceKind: SourceKind = "composer") {
    if (!text.trim()) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const item = await importText(text, id, sourceKind);
      updateItems((current) => {
        const next = [item, ...current.filter((entry) => entry.id !== item.id)];
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
      const { toast } = await import("sonner");
      toast.success(t("savedToStream"), {
        style: {
          background: "#1b2433",
          color: "#fff",
          border: "1px solid #364052",
          borderRadius: "8px",
        },
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("textImportFailed"));
    } finally {
      setIsImporting(false);
    }
  }

  async function addPaths(paths: string[], sourceKind: SourceKind = "file_picker") {
    if (!paths.length) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const imported = await importPaths(paths, sourceKind);
      updateItems((current) => {
        const next = new Map(current.map((entry) => [entry.id, entry]));
        for (const item of imported) {
          next.set(item.id, item);
        }
        const values = Array.from(next.values());
        setSyncStatus((status) => ({ ...status, pending_entries: values.length }));
        return values;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("fileImportFailed"));
    } finally {
      setIsImporting(false);
    }
  }

  async function copyText(itemId: string) {
    setError(null);
    try {
      await copyItemText(itemId);
      const { toast } = await import("sonner");
      toast.success(t("copiedToClipboard"), {
        style: {
          background: "#1b2433",
          color: "#fff",
          border: "1px solid #364052",
          borderRadius: "8px",
        },
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("failedToCopyText"));
    }
  }

  async function addConversationBundle(payload: CreateConversationBundleInput) {
    if (!payload.transcript_markdown.trim()) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const item = await importConversationBundle(payload);
      updateItems((current) => {
        const next = [item, ...current.filter((entry) => entry.id !== item.id)];
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
      const { toast } = await import("sonner");
      toast.success(t("bundleSavedToStream"), {
        style: {
          background: "#1b2433",
          color: "#fff",
          border: "1px solid #364052",
          borderRadius: "8px",
        },
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("bundleCreateFailed"));
      throw err;
    } finally {
      setIsImporting(false);
    }
  }

  async function removeItem(itemId: string) {
    try {
      await deleteItem(itemId);
      updateItems((current) => {
        const next = current.filter((entry) => entry.id !== itemId);
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("deleteFailed"));
    }
  }

  async function removeAllItems() {
    if (!items.length) return;
    try {
      for (const item of items) {
        await deleteItem(item.id);
      }
      itemsRef.current = [];
      setItems([]);
      setSyncStatus((status) => ({ ...status, pending_entries: 0 }));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("bulkDeleteFailed"));
    }
  }

  async function downloadItem(itemId: string) {
    try {
      await exportItemToDownloads(itemId);
      const { toast } = await import("sonner");
      toast.success(t("savedToDownloads"), {
        style: {
          background: "#1b2433",
          color: "#fff",
          border: "1px solid #364052",
          borderRadius: "8px",
        },
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("downloadFailed"));
    }
  }

  async function openLocalItem(itemId: string) {
    setError(null);
    try {
      await openItem(itemId);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("openItemFailed"));
    }
  }

  async function updateIntentState(itemId: string, intentState: IntentState) {
    setError(null);
    try {
      const updated = await updateItemIntentState(itemId, intentState);
      if (!updated) {
        return;
      }
      updateItems((current) => {
        const next = [updated, ...current.filter((entry) => entry.id !== updated.id)].sort(
          (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
        );
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("intentUpdateFailed"));
    }
  }

  async function setToken(newToken: string) {
    if (!newToken.trim() || newToken === syncStatus.pairing_token) return;
    try {
      const { setPairingToken } = await import("../lib/api");
      await setPairingToken(newToken);
      setSyncStatus((s) => ({ ...s, pairing_token: newToken }));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("failedToUpdateToken"));
    }
  }

  async function resetPairing() {
    try {
      const { resetPairingToken } = await import("../lib/api");
      const newToken = await resetPairingToken();
      setSyncStatus((s) => ({ ...s, pairing_token: newToken }));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("failedToResetToken"));
    }
  }

  async function unpair() {
    const confirmed = await confirmAction(t("unpairMessage"), {
      title: t("unpairTitle"),
      confirmLabel: t("unpairConfirm"),
      cancelLabel: t("cancel"),
      destructive: true,
    });

    if (!confirmed) {
      return;
    }
    try {
      const { unpairDevice } = await import("../lib/api");
      await unpairDevice();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("failedToUnpair"));
    }
  }

  async function removePairedDevice(device: PairDevice) {
    if (!syncStatus.pairing_token || !syncStatus.device_id) {
      return;
    }

    const confirmed = await confirmAction(t("removeDeviceMessage", { label: device.label }), {
      title: t("removeDeviceTitle"),
      confirmLabel: t("removeDeviceConfirm"),
      cancelLabel: t("cancel"),
      destructive: true,
    });

    if (!confirmed) {
      return;
    }

    try {
      directTransferRef.current?.disconnectPeer(device.deviceId);
      const nextStatus = await removePairingDevice({
        token: syncStatus.pairing_token,
        deviceId: syncStatus.device_id,
        targetDeviceId: device.deviceId,
      });
      setPairedDevices(nextStatus.devices);
      setSyncStatus((current) => {
        const linkedDevices = Math.max(0, nextStatus.pairedDeviceCount - 1);
        return {
          ...current,
          paired_devices: linkedDevices,
          transport: resolveTransportStatus(networkMode, linkedDevices, directConnectedRef.current),
        };
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t("failedToRemovePairedDevice"));
    }
  }

  return {
    error,
    isHydrating,
    isImporting,
    items: sortedItems,
    syncStatus,
    pairedDevices,
    relayUploadProgress,
    networkMode,
    setNetworkMode: networkModeStore.set,
    addText,
    addPaths,
    addConversationBundle,
    copyText,
    removeItem,
    removeAllItems,
    downloadItem,
    openLocalItem,
    updateIntentState,
    setToken,
    resetPairing,
    unpair,
    removePairedDevice,
  };
}
