import { convertFileSrc } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { Item } from "../lib/types";

type ItemCardProps = {
  item: Item;
  onCopyText: (itemId: string) => void;
  onDeleteItem: (itemId: string) => void;
  onDownloadItem: (itemId: string, destinationPath: string) => void;
};

export function ItemCard({ item, onCopyText, onDeleteItem, onDownloadItem }: ItemCardProps) {
  const isText = item.type === "text";
  const isImage = item.type === "image";
  const label = item.name ?? item.type;

  async function handleDownload() {
    const fallbackName = isText ? "dropply-note.txt" : item.name ?? `dropply-${item.type}`;
    const destination = await save({
      defaultPath: fallbackName,
    });

    if (!destination) {
      return;
    }

    await onDownloadItem(item.id, destination);
  }

  return (
    <article className={`item-card item-card--${item.type}`}>
      <div className="item-meta">
        <span className="item-type-chip">{item.type}</span>
        <span>{new Date(item.updated_at).toLocaleString()}</span>
      </div>
      <div className="item-content">
        <strong className="item-title">{label}</strong>
        {isText ? <p>{item.text_preview}</p> : null}
        {isImage ? (
          <img
            src={convertFileSrc(item.content_ref, "asset")}
            alt={item.name ?? "Dropped image"}
            loading="lazy"
          />
        ) : null}
        {!isText && !isImage ? (
          <div className="file-tile">
            <strong>{item.name ?? "File"}</strong>
            <span>{item.mime_type ?? "application/octet-stream"}</span>
            <span>{formatBytes(item.size_bytes ?? 0)}</span>
          </div>
        ) : null}
      </div>
      <div className="item-actions">
        <div className="item-action-group">
          {isText ? (
            <button type="button" className="ghost" onClick={() => onCopyText(item.id)}>
              Copy
            </button>
          ) : null}
          <button type="button" className="ghost" onClick={() => void handleDownload()}>
            Download
          </button>
          <button type="button" className="ghost destructive" onClick={() => onDeleteItem(item.id)}>
            Delete
          </button>
        </div>
      </div>
    </article>
  );
}

function formatBytes(size: number) {
  if (size < 1024) {
    return `${size} B`;
  }
  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`;
  }
  if (size < 1024 * 1024 * 1024) {
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
