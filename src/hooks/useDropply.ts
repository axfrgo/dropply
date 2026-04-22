import { useEffect, useMemo, useState, useRef } from "react";
import {
  bootstrapApp,
  copyItemText,
  deleteItem,
  exportItem,
  exportRelayItems,
  importPaths,
  importText,
} from "../lib/api";
import type { BootstrapPayload, Item, SyncStatus } from "../lib/types";
import { fetchPairStatus, pushDesktopRelaySnapshot, registerPairingDevice } from "../lib/relay";

const EMPTY_STATUS: SyncStatus = {
  device_id: "booting",
  paired_devices: 0,
  transport: "offline",
  relay_connected: false,
  pending_entries: 0,
  pairing_token: "",
};

export function useDropply() {
  const [items, setItems] = useState<Item[]>([]);
  const [syncStatus, setSyncStatus] = useState<SyncStatus>(EMPTY_STATUS);
  const p2pConns = useRef<any[]>([]);
  const [isHydrating, setIsHydrating] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    bootstrapApp()
      .then((payload: BootstrapPayload) => {
        if (!isMounted) {
          return;
        }
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
          setError(err instanceof Error ? err.message : "Failed to boot Dropply.");
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
    if (!syncStatus.pairing_token || !syncStatus.device_id || syncStatus.device_id === "booting") {
      return;
    }

    let cancelled = false;

    async function syncPairState() {
      try {
        const registration = await registerPairingDevice({
          token: syncStatus.pairing_token,
          deviceId: syncStatus.device_id,
          deviceType: "desktop",
          label: "Dropply desktop",
        });

        if (cancelled) {
          return;
        }

        const linkedDevices = Math.max(0, registration.pairedDeviceCount - 1);
        setSyncStatus((current) => ({
          ...current,
          paired_devices: linkedDevices,
          transport: linkedDevices > 0 ? "relay" : "offline",
        }));

        if (linkedDevices > 0) {
          await exportRelayItems();
          await pushDesktopRelaySnapshot({
            token: syncStatus.pairing_token,
            deviceId: syncStatus.device_id,
          });
        }

        const latest = await fetchPairStatus(syncStatus.pairing_token, syncStatus.device_id);
        if (cancelled) {
          return;
        }
        const latestLinkedDevices = Math.max(0, latest.pairedDeviceCount - 1);
        setSyncStatus((current) => ({
          ...current,
          paired_devices: latestLinkedDevices,
          transport: latestLinkedDevices > 0 ? "relay" : "offline",
        }));
      } catch {
        if (!cancelled) {
          setSyncStatus((current) => ({
            ...current,
            paired_devices: 0,
            transport: "offline",
          }));
        }
      }
    }

    void syncPairState();
    const timer = window.setInterval(() => {
      void syncPairState();
    }, 4000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [items, syncStatus.device_id, syncStatus.pairing_token]);

  const sortedItems = useMemo(
    () =>
      [...items].sort((a, b) =>
        a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : 0,
      ),
    [items],
  );

  useEffect(() => {
    if (!syncStatus.device_id || syncStatus.device_id === "booting") {
      return;
    }

    let peer: any = null;

    async function initP2P() {
      try {
        const { default: Peer } = await import("peerjs");
        peer = new Peer(`dropply-${syncStatus.device_id}`);

        peer.on("connection", (conn: any) => {
          conn.on("open", async () => {
            p2pConns.current.push(conn);
            setSyncStatus((current) => ({ ...current, transport: "lan" }));
            const relayItems = await exportRelayItems();
            conn.send(relayItems);
          });

          conn.on("close", () => {
            p2pConns.current = p2pConns.current.filter((c) => c !== conn);
          });
        });
      } catch (err) {
        console.error("PeerJS initialization failed", err);
      }
    }

    void initP2P();

    return () => {
      p2pConns.current = [];
      peer?.destroy();
    };
  }, [syncStatus.device_id]);

  useEffect(() => {
    if (p2pConns.current.length === 0) return;
    exportRelayItems().then((relayItems) => {
      for (const conn of p2pConns.current) {
        conn.send(relayItems);
      }
    });
  }, [items]);

  async function addText(text: string) {
    if (!text.trim()) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const item = await importText(text);
      setItems((current) => {
        const next = [item, ...current.filter((entry) => entry.id !== item.id)];
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Text import failed.");
    } finally {
      setIsImporting(false);
    }
  }

  async function addPaths(paths: string[]) {
    if (!paths.length) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const imported = await importPaths(paths);
      setItems((current) => {
        const next = new Map(current.map((entry) => [entry.id, entry]));
        for (const item of imported) {
          next.set(item.id, item);
        }
        const values = Array.from(next.values());
        setSyncStatus((status) => ({ ...status, pending_entries: values.length }));
        return values;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "File import failed.");
    } finally {
      setIsImporting(false);
    }
  }

  async function copyText(itemId: string) {
    try {
      await copyItemText(itemId);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Copy failed.");
    }
  }

  async function removeItem(itemId: string) {
    try {
      await deleteItem(itemId);
      setItems((current) => {
        const next = current.filter((entry) => entry.id !== itemId);
        setSyncStatus((status) => ({ ...status, pending_entries: next.length }));
        return next;
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Delete failed.");
    }
  }

  async function downloadItem(itemId: string, destinationPath: string) {
    try {
      await exportItem(itemId, destinationPath);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Download failed.");
    }
  }

  return {
    error,
    isHydrating,
    isImporting,
    items: sortedItems,
    syncStatus,
    addText,
    addPaths,
    copyText,
    removeItem,
    downloadItem,
  };
}
