import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, type PhysicalPosition } from "@tauri-apps/api/window";
import { AuthModal } from "./components/AuthModal";
import { Canvas } from "./components/Canvas";
import { PairingStrip } from "./components/PairingStrip";
import { useDropply } from "./hooks/useDropply";

export default function App() {
  const {
    error,
    isHydrating,
    isImporting,
    items,
    syncStatus,
    addPaths,
    addText,
    copyText,
    removeItem,
    downloadItem,
  } = useDropply();
  const [isPinned, setIsPinned] = useState(false);
  const [authMode, setAuthMode] = useState<"signin" | "signup" | null>(null);
  const isPinnedRef = useRef(false);
  const pinnedPositionRef = useRef<PhysicalPosition | null>(null);
  const restoringPositionRef = useRef(false);

  useEffect(() => {
    isPinnedRef.current = isPinned;
  }, [isPinned]);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let isMounted = true;
    let unlisten: (() => void) | undefined;

    void Promise.all([invoke<boolean>("get_window_pin_state"), appWindow.outerPosition()])
      .then(([alwaysOnTop, position]) => {
        if (!isMounted) {
          return;
        }
        isPinnedRef.current = alwaysOnTop;
        setIsPinned(alwaysOnTop);
        pinnedPositionRef.current = position;
      })
      .catch(() => {
        if (isMounted) {
          setIsPinned(false);
        }
      });

    void appWindow
      .onMoved(async ({ payload }) => {
        if (!isMounted || !isPinnedRef.current || !pinnedPositionRef.current) {
          return;
        }

        if (restoringPositionRef.current) {
          restoringPositionRef.current = false;
          return;
        }

        const anchor = pinnedPositionRef.current;
        if (payload.x === anchor.x && payload.y === anchor.y) {
          return;
        }

        restoringPositionRef.current = true;
        await appWindow.setPosition(anchor);
      })
      .then((dispose) => {
        unlisten = dispose;
      });

    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, []);

  async function togglePinned() {
    const appWindow = getCurrentWindow();
    const next = !isPinned;
    const previousAnchor = pinnedPositionRef.current;

    if (next) {
      pinnedPositionRef.current = await appWindow.outerPosition();
    } else {
      pinnedPositionRef.current = null;
    }

    isPinnedRef.current = next;
    setIsPinned(next);

    try {
      await invoke("set_window_pin_state", { pinned: next });
    } catch {
      pinnedPositionRef.current = previousAnchor;
      isPinnedRef.current = !next;
      setIsPinned(!next);
      return;
    }

    if (next) {
      try {
        await appWindow.setFocus();
      } catch {
        // Focus failures should not undo a successful pin state change.
      }
    }
  }

  async function openExternalUrl(url: string) {
    await invoke("open_external_url", { url });
  }

  const syncLabel = syncStatus.transport === "offline" ? "local only" : syncStatus.transport;
  const linkLabel =
    syncStatus.transport === "offline" ? "sync not live" : `${syncStatus.paired_devices} linked`;
  const pendingLabel =
    syncStatus.transport === "offline"
      ? `${syncStatus.pending_entries} items`
      : `${syncStatus.pending_entries} pending`;

  return (
    <div className="app-shell">
      <div className="ambient ambient--left" />
      <div className="ambient ambient--right" />

      <header className="status-bar">
        <div className="status-brand">
          <div className="brand-mark" aria-hidden="true">
            <span />
          </div>
          <div>
            <span className="brand">Dropply</span>
            <p className="brand-subtitle">Shared scratchpad for everything in motion</p>
          </div>
        </div>
        <div className="status-actions">
          <div className="status-group">
            <span className="status-pill">{syncLabel}</span>
            {isPinned ? <span className="status-pill">pinned</span> : null}
            <span>{linkLabel}</span>
            <span>{pendingLabel}</span>
          </div>
          <button type="button" className="composer-tool" onClick={() => setAuthMode("signin")}>
            Sign in
          </button>
          <button type="button" className="composer-send" onClick={() => setAuthMode("signup")}>
            Sign up
          </button>
          <button
            type="button"
            className={`composer-tool ${isPinned ? "is-active" : ""}`}
            onClick={() => void togglePinned()}
          >
            {isPinned ? "Unpin window" : "Pin window"}
          </button>
        </div>
      </header>

      <PairingStrip syncStatus={syncStatus} />

      {error ? <div className="error-banner">{error}</div> : null}
      {isHydrating ? (
        <div className="loading-state">Loading your scratchpad...</div>
      ) : (
        <Canvas
          items={items}
          isBusy={isImporting}
          onAddPaths={addPaths}
          onAddText={addText}
          onCopyText={copyText}
          onDeleteItem={removeItem}
          onDownloadItem={downloadItem}
        />
      )}

      {authMode ? (
        <AuthModal
          mode={authMode}
          onClose={() => setAuthMode(null)}
          onOpenExternalUrl={openExternalUrl}
        />
      ) : null}
    </div>
  );
}
