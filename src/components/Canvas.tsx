import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { confirmAction } from "../lib/confirm";
import { useI18n } from "../lib/i18n";
import type { IntentState, Item, SourceKind } from "../lib/types";
import { ItemCard } from "./ItemCard";

type CanvasProps = {
  items: Item[];
  isBusy: boolean;
  onAddPaths: (paths: string[], sourceKind?: SourceKind) => Promise<void>;
  onAddText: (text: string, id?: string, sourceKind?: SourceKind) => Promise<void>;
  onOpenBundleComposer: () => void;
  onCopyText: (itemId: string) => Promise<void>;
  onDeleteItem: (itemId: string) => Promise<void>;
  onDeleteAllItems: () => Promise<void>;
  onDownloadItem: (itemId: string) => Promise<void>;
  onOpenItem: (itemId: string) => Promise<void>;
  onUpdateIntentState: (itemId: string, intentState: IntentState) => Promise<void>;
  canSendToDevice: boolean;
};

const VISIBLE_COUNT = 60;

export function Canvas({
  items,
  isBusy,
  onAddPaths,
  onAddText,
  onOpenBundleComposer,
  onCopyText,
  onDeleteItem,
  onDeleteAllItems,
  onDownloadItem,
  onOpenItem,
  onUpdateIntentState,
  canSendToDevice,
}: CanvasProps) {
  const { t } = useI18n();
  const [isDragging, setIsDragging] = useState(false);
  const [draftText, setDraftText] = useState("");
  const [draftSourceKind, setDraftSourceKind] = useState<SourceKind>("composer");
  const [composerMode, setComposerMode] = useState<"idle" | "typing">("idle");

  const visibleItems = useMemo(() => items.slice(0, VISIBLE_COUNT), [items]);

  useEffect(() => {
    function handleWindowPaste(event: ClipboardEvent) {
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
        return;
      }

      void importClipboardPayload(event);
    }

    window.addEventListener("paste", handleWindowPaste);
    return () => window.removeEventListener("paste", handleWindowPaste);
  });

  async function handleDrop(event: React.DragEvent<HTMLElement>) {
    event.preventDefault();
    setIsDragging(false);

    const filePaths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));

    if (filePaths.length) {
      await onAddPaths(filePaths, "drag_drop");
      return;
    }

    const text = event.dataTransfer.getData("text/plain");
    if (text) {
      await onAddText(text, undefined, "drag_drop");
    }
  }

  async function importClipboardPayload(event: ClipboardEvent | React.ClipboardEvent<HTMLElement>) {
    const clipboardData = event.clipboardData;
    if (!clipboardData) {
      return;
    }

    const clipboardItems = Array.from(clipboardData.items);
    const text = clipboardData.getData("text/plain");

    const pastedFiles = clipboardItems
      .filter((item) => item.kind === "file")
      .map((item) => item.getAsFile())
      .filter((file): file is File & { path?: string } => Boolean(file))
      .map((file) => file.path)
      .filter((path): path is string => Boolean(path));

    if (pastedFiles.length) {
      event.preventDefault();
      await onAddPaths(pastedFiles, "paste");
      return;
    }

    if (text.trim()) {
      event.preventDefault();
      setComposerMode("typing");
      setDraftText(text);
      setDraftSourceKind("paste");
    }
  }

  async function handlePickFiles() {
    const selection = await open({
      multiple: true,
      directory: false,
    });

    if (!selection) {
      return;
    }

    const paths = Array.isArray(selection) ? selection : [selection];
    await onAddPaths(paths, "file_picker");
  }

  async function handlePasteButton() {
    try {
      const text = await navigator.clipboard.readText();
      setComposerMode("typing");
      setDraftText(text);
      setDraftSourceKind("paste");
    } catch {
      setComposerMode("typing");
      setDraftSourceKind("paste");
    }
  }

  async function handleSubmitText() {
    const text = draftText.trim();
    if (!text) {
      return;
    }

    await onAddText(text, undefined, draftSourceKind);
    setDraftText("");
    setDraftSourceKind("composer");
    setComposerMode("idle");
  }

  return (
    <main
      className={`canvas ${isDragging ? "canvas--dragging" : ""}`}
      onDragEnter={(event) => {
        event.preventDefault();
        setIsDragging(true);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node | null)) {
          return;
        }
        setIsDragging(false);
      }}
      onDrop={handleDrop}
      onPaste={(event) => {
        void importClipboardPayload(event);
      }}
    >
      <section className="hero">
        <div className="hero-copy-wrap">
          <p className="eyebrow">{t("localFirstCanvas")}</p>
          <h1>{t("heroTitle")}</h1>
          <p className="hero-copy">{t("heroCopy")}</p>
        </div>
      </section>

      <section className="composer-shell">
        <div className="composer-card">
          <textarea
            className="composer-input"
            placeholder={t("composerPlaceholder")}
            value={draftText}
            onChange={(event) => {
              setComposerMode("typing");
              setDraftSourceKind("composer");
              setDraftText(event.target.value);
            }}
            onFocus={() => setComposerMode("typing")}
            onKeyDown={(event) => {
              if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                void handleSubmitText();
              }
            }}
          />
          <div className="composer-actions">
            <div className="composer-actions-left">
              <button type="button" className="composer-tool" onClick={handlePickFiles}>
                {t("addFiles")}
              </button>
              <button type="button" className="composer-tool" onClick={handlePasteButton}>
                {t("paste")}
              </button>
              <button type="button" className="composer-tool" onClick={onOpenBundleComposer}>
                {t("createBundle")}
              </button>
              {items.length > 0 && (
                <button
                  type="button"
                  className="composer-tool"
                  onClick={() => {
                    void confirmAction(t("clearStreamMessage"), {
                      title: t("clearStream"),
                      confirmLabel: t("clearStreamConfirm"),
                      cancelLabel: t("cancel"),
                      destructive: true,
                    }).then((confirmed) => {
                      if (confirmed) {
                        void onDeleteAllItems();
                      }
                    });
                  }}
                  style={{ color: "#ff4d4d" }}
                >
                  {t("clearStream")}
                </button>
              )}
            </div>
            <button
              type="button"
              className="composer-send"
              onClick={() => void handleSubmitText()}
              disabled={!draftText.trim()}
            >
              {t("sendToStream")}
            </button>
          </div>
          <div className="composer-hint">
            {composerMode === "typing" ? t("typingHint") : t("idleHint")}
          </div>
        </div>
      </section>

      <section className="stream" aria-live="polite">
        {visibleItems.map((item) => (
          <ItemCard
            key={item.id}
            item={item}
            onCopyText={onCopyText}
            onDeleteItem={onDeleteItem}
            onDownloadItem={onDownloadItem}
            onOpenItem={onOpenItem}
            onUpdateIntentState={onUpdateIntentState}
            canSendToDevice={canSendToDevice}
          />
        ))}
        {!visibleItems.length ? (
          <div className="empty-state">
            <p>{t("emptyTitle")}</p>
            <span>{t("emptyCopy")}</span>
          </div>
        ) : null}
      </section>

      {isBusy ? <div className="busy-indicator">{t("importing")}</div> : null}
    </main>
  );
}
