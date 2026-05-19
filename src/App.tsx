import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, type PhysicalPosition } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { Toaster, toast } from "sonner";
import { AuthModal } from "./components/AuthModal";
import { Canvas } from "./components/Canvas";
import { ConversationBundleComposerModal } from "./components/ConversationBundleComposerModal";
import { DesktopMenuBar } from "./components/DesktopMenuBar";
import { PairingStrip } from "./components/PairingStrip";
import { UpdateNotice } from "./components/UpdateNotice";
import {
  announceDesktopUpdatePreviewChange,
  UPDATE_PREVIEW_URL_KEY,
  UPDATE_PREVIEW_VERSION_KEY,
  useDesktopUpdateSignal,
} from "./hooks/useDesktopUpdateSignal";
import { useDropply } from "./hooks/useDropply";
import { fetchCloudConfig } from "./lib/cloud";
import { confirmAction } from "./lib/confirm";
import { I18nProvider, useI18n } from "./lib/i18n";
import { cloudAuthAvailableStore, useCloudAuthAvailableStore } from "./lib/preferences";

export default function App() {
  return (
    <I18nProvider>
      <AppShell />
    </I18nProvider>
  );
}

function nextPatchVersion(version: string | null) {
  const normalized = (version ?? "1.0.0").replace(/^v/i, "");
  const parts = normalized.split(".").map((part) => Number.parseInt(part, 10));
  const major = Number.isFinite(parts[0]) ? parts[0] : 1;
  const minor = Number.isFinite(parts[1]) ? parts[1] : 0;
  const patch = Number.isFinite(parts[2]) ? parts[2] + 1 : 1;
  return `${major}.${minor}.${patch}`;
}

type ShellSize = "wide" | "desktop" | "split" | "compact" | "narrow";

function classifyShellSize(width: number): ShellSize {
  if (width < 720) {
    return "narrow";
  }

  if (width < 920) {
    return "compact";
  }

  if (width < 1320) {
    return "split";
  }

  if (width < 1700) {
    return "desktop";
  }

  return "wide";
}

function clearUpdatePreviewSignal() {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.removeItem(UPDATE_PREVIEW_VERSION_KEY);
  window.localStorage.removeItem(UPDATE_PREVIEW_URL_KEY);
  announceDesktopUpdatePreviewChange();
}

function enableUpdatePreviewSignal(currentVersion: string | null) {
  if (typeof window === "undefined") {
    return null;
  }

  const previewVersion = nextPatchVersion(currentVersion);
  window.localStorage.setItem(UPDATE_PREVIEW_VERSION_KEY, previewVersion);
  window.localStorage.setItem(UPDATE_PREVIEW_URL_KEY, "https://dropply.ca/download");
  announceDesktopUpdatePreviewChange();
  return previewVersion;
}

function AppShell() {
  const { locale, setLocale, t } = useI18n();
  const {
    error,
    isHydrating,
    isImporting,
    items,
    syncStatus,
    pairedDevices,
    relayUploadProgress,
    networkMode,
    setNetworkMode,
    addPaths,
    addText,
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
  } = useDropply();
  const [isPinned, setIsPinned] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);
  const [authMode, setAuthMode] = useState<"signin" | "signup" | null>(null);
  const [isBundleComposerOpen, setIsBundleComposerOpen] = useState(false);
  const [shellSize, setShellSize] = useState<ShellSize>(() =>
    typeof window === "undefined" ? "wide" : classifyShellSize(window.innerWidth),
  );
  const cloudAuthAvailable = useCloudAuthAvailableStore();
  const updateSignal = useDesktopUpdateSignal();
  const isPinnedRef = useRef(false);
  const pinnedPositionRef = useRef<PhysicalPosition | null>(null);
  const restoringPositionRef = useRef(false);

  useEffect(() => {
    isPinnedRef.current = isPinned;
  }, [isPinned]);

  useEffect(() => {
    let isMounted = true;

    void fetchCloudConfig()
      .then((config) => {
        if (isMounted) {
          cloudAuthAvailableStore.set(Boolean(config.auth_configured && config.hosted_sync_available));
        }
      })
      .catch(() => {
        if (isMounted) {
          cloudAuthAvailableStore.set(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    function syncShellSize() {
      setShellSize(classifyShellSize(window.innerWidth));
    }

    syncShellSize();
    window.addEventListener("resize", syncShellSize);
    return () => window.removeEventListener("resize", syncShellSize);
  }, []);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let isMounted = true;
    let unlisten: (() => void) | undefined;
    let unlistenResized: (() => void) | undefined;

    void Promise.all([
      invoke<boolean>("get_window_pin_state"),
      appWindow.outerPosition(),
      appWindow.isMaximized(),
    ])
      .then(([alwaysOnTop, position, maximized]) => {
        if (!isMounted) {
          return;
        }
        isPinnedRef.current = alwaysOnTop;
        setIsPinned(alwaysOnTop);
        pinnedPositionRef.current = position;
        setIsMaximized(maximized);
      })
      .catch(() => {
        if (isMounted) {
          setIsPinned(false);
          setIsMaximized(false);
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

    void appWindow
      .onResized(async () => {
        if (!isMounted) {
          return;
        }

        try {
          setIsMaximized(await appWindow.isMaximized());
        } catch {
          // Ignore maximize state polling failures.
        }
      })
      .then((dispose) => {
        unlistenResized = dispose;
      });

    return () => {
      isMounted = false;
      unlisten?.();
      unlistenResized?.();
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || !event.shiftKey || event.key.toLowerCase() !== "u") {
        return;
      }

      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) {
        return;
      }

      if (typeof window === "undefined") {
        return;
      }

      event.preventDefault();

      if (event.altKey) {
        clearUpdatePreviewSignal();
        toast.success(t("updatePreviewCleared"));
        return;
      }

      const previewVersion = enableUpdatePreviewSignal(updateSignal.currentVersion);
      if (previewVersion) {
        toast.success(t("updatePreviewEnabled", { version: previewVersion }));
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [t, updateSignal.currentVersion]);

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

  async function openFilePicker() {
    const selection = await open({
      multiple: true,
      directory: false,
    });

    if (!selection) {
      return;
    }

    const paths = Array.isArray(selection) ? selection : [selection];
    await addPaths(paths);
  }

  async function clearStreamFromMenu() {
    const confirmed = await confirmAction(t("clearStreamMessage"), {
      title: t("clearStream"),
      confirmLabel: t("clearStreamConfirm"),
      cancelLabel: t("cancel"),
      destructive: true,
    });

    if (confirmed) {
      await removeAllItems();
    }
  }

  async function copyPairingCode() {
    await navigator.clipboard.writeText(syncStatus.pairing_token);
    toast.success(t("copiedToClipboard"));
  }

  async function toggleUpdatePreview() {
    if (updateSignal.visible) {
      clearUpdatePreviewSignal();
      toast.success(t("updatePreviewCleared"));
      return;
    }

    const previewVersion = enableUpdatePreviewSignal(updateSignal.currentVersion);
    if (previewVersion) {
      toast.success(t("updatePreviewEnabled", { version: previewVersion }));
    }
  }

  async function startWindowDrag() {
    await invoke("start_window_drag");
  }

  async function minimizeWindow() {
    await invoke("minimize_window");
  }

  async function toggleMaximizeWindow() {
    const maximized = await invoke<boolean>("toggle_maximize_window");
    setIsMaximized(maximized);
  }

  async function closeWindow() {
    await invoke("close_window");
  }

  async function createConversationBundle(payload: Parameters<typeof addConversationBundle>[0]) {
    await addConversationBundle(payload);
  }

  const syncLabel = syncStatus.transport === "offline" ? t("localOnly") : syncStatus.transport;
  const linkLabel =
    syncStatus.transport === "offline"
      ? t("syncNotLive")
      : t("linked", { count: syncStatus.paired_devices });
  const pendingLabel =
    syncStatus.transport === "offline"
      ? t("items", { count: syncStatus.pending_entries })
      : t("pending", { count: syncStatus.pending_entries });

  return (
    <div className="app-shell" data-shell-size={shellSize}>
      <Toaster position="bottom-right" theme="dark" toastOptions={{
        style: {
          background: "#1b2433",
          color: "#fff",
          border: "1px solid #364052",
        }
      }} />
      <div className="ambient ambient--left" />
      <div className="ambient ambient--right" />

      <DesktopMenuBar
        title="Dropply"
        labels={{
          file: t("menuFile"),
          edit: t("menuEdit"),
          view: t("menuView"),
          window: t("menuWindow"),
          help: t("menuHelp"),
          addFiles: t("addFiles"),
          createBundle: t("createBundle"),
          copyCode: t("copyCode"),
          reset: t("reset"),
          clearStream: t("clearStream"),
          pinWindow: t("pinWindow"),
          unpinWindow: t("unpinWindow"),
          showUpdatePreview: t("showUpdatePreview"),
          hideUpdatePreview: t("hideUpdatePreview"),
          minimizeWindow: t("minimizeWindow"),
          maximizeWindow: t("maximizeWindow"),
          restoreWindow: t("restoreWindow"),
          closeWindow: t("closeWindow"),
          openWebsite: t("openWebsite"),
          openDownloads: t("openDownloads"),
          quitDropply: t("quitDropply"),
        }}
        isPinned={isPinned}
        isMaximized={isMaximized}
        updatePreviewVisible={updateSignal.visible}
        onStartDrag={startWindowDrag}
        onOpenFiles={openFilePicker}
        onCreateBundle={() => setIsBundleComposerOpen(true)}
        onCopyCode={copyPairingCode}
        onResetPairing={resetPairing}
        onClearStream={clearStreamFromMenu}
        onTogglePin={togglePinned}
        onToggleUpdatePreview={toggleUpdatePreview}
        onOpenWebsite={() => openExternalUrl("https://dropply.ca")}
        onOpenDownloads={() => openExternalUrl("https://dropply.ca/download")}
        onMinimize={minimizeWindow}
        onToggleMaximize={toggleMaximizeWindow}
        onClose={closeWindow}
      />

      <header className="status-bar">
        <div className="status-bar__update-slot">
          {updateSignal.visible && updateSignal.availableVersion ? (
            <UpdateNotice
              availableVersion={updateSignal.availableVersion}
              currentVersion={updateSignal.currentVersion}
              onOpen={() => openExternalUrl(updateSignal.downloadUrl)}
              title={t("updateReadyTitle")}
              subtitle={t("updateReadySubtitle", { version: updateSignal.availableVersion })}
              ctaLabel={t("downloadUpdate")}
            />
          ) : (
            <div className="status-bar__idle-mark" aria-hidden="true">
              <span className="status-bar__idle-mark-orb" />
            </div>
          )}
        </div>
        <div className="status-actions">
          <div className="status-group status-group--locale">
            <button
              type="button"
              className={`composer-tool ${locale === "en" ? "is-active" : ""}`}
              onClick={() => setLocale("en")}
            >
              EN
            </button>
            <button
              type="button"
              className={`composer-tool ${locale === "fr" ? "is-active" : ""}`}
              onClick={() => setLocale("fr")}
            >
              FR
            </button>
          </div>
          <div className="status-group status-group--overview">
            <span className="status-pill">{syncLabel}</span>
            {isPinned ? <span className="status-pill">{t("pinned")}</span> : null}
            <span>{linkLabel}</span>
            <span>{pendingLabel}</span>
          </div>
          <div className="status-bar__utility">
            {cloudAuthAvailable ? (
              <>
                <button type="button" className="composer-tool" onClick={() => setAuthMode("signin")}>
                  {t("signIn")}
                </button>
                <button type="button" className="composer-send" onClick={() => setAuthMode("signup")}>
                  {t("signUp")}
                </button>
              </>
            ) : null}
            <button
              type="button"
              className={`composer-tool ${isPinned ? "is-active" : ""}`}
              onClick={() => void togglePinned()}
            >
              {isPinned ? t("unpinWindow") : t("pinWindow")}
            </button>
          </div>
        </div>
      </header>

      <div className="app-scroll-region">
        <PairingStrip 
          syncStatus={syncStatus} 
          pairedDevices={pairedDevices}
          relayUploadProgress={relayUploadProgress}
          onSetToken={setToken} 
          networkMode={networkMode}
          onToggleNetwork={setNetworkMode}
          onResetToken={resetPairing}
          onUnpair={unpair}
          onRemoveDevice={removePairedDevice}
        />

        {error ? <div className="error-banner">{error}</div> : null}
        {isHydrating ? (
          <div className="loading-state">{t("loadingScratchpad")}</div>
        ) : (
          <Canvas
            items={items}
            isBusy={isImporting}
            onAddPaths={addPaths}
            onAddText={addText}
            onOpenBundleComposer={() => setIsBundleComposerOpen(true)}
            onCopyText={copyText}
            onDeleteItem={removeItem}
            onDeleteAllItems={removeAllItems}
            onDownloadItem={downloadItem}
            onOpenItem={openLocalItem}
            onUpdateIntentState={updateIntentState}
            canSendToDevice={syncStatus.paired_devices > 0}
          />
        )}
      </div>

      {authMode ? (
        <AuthModal
          mode={authMode}
          onClose={() => setAuthMode(null)}
          onOpenExternalUrl={openExternalUrl}
        />
      ) : null}
      {isBundleComposerOpen ? (
        <ConversationBundleComposerModal
          isBusy={isImporting}
          onClose={() => setIsBundleComposerOpen(false)}
          onCreateBundle={createConversationBundle}
        />
      ) : null}
    </div>
  );
}
