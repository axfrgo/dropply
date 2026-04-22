import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Item } from "../lib/types";
import { ItemCard } from "./ItemCard";

type CanvasProps = {
  items: Item[];
  isBusy: boolean;
  onAddPaths: (paths: string[]) => Promise<void>;
  onAddText: (text: string) => Promise<void>;
  onCopyText: (itemId: string) => Promise<void>;
  onDeleteItem: (itemId: string) => Promise<void>;
  onDownloadItem: (itemId: string, destinationPath: string) => Promise<void>;
};

const VISIBLE_COUNT = 60;

export function Canvas({
  items,
  isBusy,
  onAddPaths,
  onAddText,
  onCopyText,
  onDeleteItem,
  onDownloadItem,
}: CanvasProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [draftText, setDraftText] = useState("");
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
      await onAddPaths(filePaths);
      return;
    }

    const text = event.dataTransfer.getData("text/plain");
    if (text) {
      await onAddText(text);
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
      await onAddPaths(pastedFiles);
      return;
    }

    if (text.trim()) {
      event.preventDefault();
      setComposerMode("typing");
      setDraftText(text);
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
    await onAddPaths(paths);
  }

  async function handlePasteButton() {
    try {
      const text = await navigator.clipboard.readText();
      setComposerMode("typing");
      setDraftText(text);
    } catch {
      setComposerMode("typing");
    }
  }

  async function handleSubmitText() {
    const text = draftText.trim();
    if (!text) {
      return;
    }

    await onAddText(text);
    setDraftText("");
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
          <p className="eyebrow">Local-first shared canvas</p>
          <h1>Drop anything. It shows up everywhere.</h1>
          <p className="hero-copy">
            Keep text, files, screenshots, and quick thoughts in one live stream without accounts
            or extra friction.
          </p>
        </div>
      </section>

      <section className="composer-shell">
        <div className="composer-card">
          <textarea
            className="composer-input"
            placeholder="Type or paste text here. Ctrl+Enter sends it to the stream."
            value={draftText}
            onChange={(event) => {
              setComposerMode("typing");
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
                Add files
              </button>
              <button type="button" className="composer-tool" onClick={handlePasteButton}>
                Paste
              </button>
            </div>
            <button
              type="button"
              className="composer-send"
              onClick={() => void handleSubmitText()}
              disabled={!draftText.trim()}
            >
              Send to stream
            </button>
          </div>
          <div className="composer-hint">
            {composerMode === "typing"
              ? "Text lands in the shared stream immediately after send."
              : "Paste anywhere or start typing here."}
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
          />
        ))}
        {!visibleItems.length ? (
          <div className="empty-state">
            <p>Nothing here yet.</p>
            <span>Drop a file, paste a screenshot, or send text from the composer above.</span>
          </div>
        ) : null}
      </section>

      {isBusy ? <div className="busy-indicator">Importing...</div> : null}
    </main>
  );
}
