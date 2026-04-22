import { useEffect, useMemo, useState } from "react";
import {
  bootstrapApp,
  copyItemText,
  deleteItem,
  exportItem,
  importPaths,
  importText,
} from "../lib/api";
import type { BootstrapPayload, Item, SyncStatus } from "../lib/types";

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
        setSyncStatus(payload.sync_status);
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

  const sortedItems = useMemo(
    () =>
      [...items].sort((a, b) =>
        a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : 0,
      ),
    [items],
  );

  async function addText(text: string) {
    if (!text.trim()) {
      return;
    }
    setIsImporting(true);
    setError(null);
    try {
      const item = await importText(text);
      setItems((current) => [item, ...current.filter((entry) => entry.id !== item.id)]);
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
        return Array.from(next.values());
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
      setItems((current) => current.filter((entry) => entry.id !== itemId));
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
