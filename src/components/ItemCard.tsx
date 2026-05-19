import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ConversationBundleModal } from "./ConversationBundleModal";
import { isConversationBundleItem } from "../lib/bundles";
import { useI18n } from "../lib/i18n";
import type { IntentState, Item, SourceContext, SuggestedAction } from "../lib/types";

type Translate = ReturnType<typeof useI18n>["t"];

type ItemCardProps = {
  item: Item;
  onCopyText: (itemId: string) => void;
  onDeleteItem: (itemId: string) => void;
  onDownloadItem: (itemId: string) => void;
  onOpenItem: (itemId: string) => void;
  onUpdateIntentState: (itemId: string, intentState: IntentState) => void;
  canSendToDevice: boolean;
};

export function ItemCard({
  item,
  onCopyText,
  onDeleteItem,
  onDownloadItem,
  onOpenItem,
  onUpdateIntentState,
  canSendToDevice,
}: ItemCardProps) {
  const { formatBytes, formatDateTime, formatItemType, t } = useI18n();
  const [isBundleOpen, setIsBundleOpen] = useState(false);
  const isText = item.type === "text";
  const isImage = item.type === "image";
  const isBundle = isConversationBundleItem(item);
  const label = item.name ?? (isBundle ? t("conversationBundle") : item.type);
  const textBody = item.text_content ?? item.text_preview;
  const intentState = item.intent_state ?? "captured";
  const semantic = item.semantic_context;
  const sourceContext = item.source_context;
  const smartActions = (item.suggested_actions ?? [])
    .filter((action) =>
      action.id === "open" ||
      action.id === "resume_later" ||
      action.id === "summarize_later" ||
      action.id === "send_to_device"
    )
    .slice(0, 4);

  async function handleDownload() {
    await onDownloadItem(item.id);
  }

  function handleSmartAction(action: SuggestedAction) {
    const actionEnabled = action.enabled || (action.id === "send_to_device" && canSendToDevice);
    if (!actionEnabled) {
      return;
    }

    if (action.id === "open") {
      onOpenItem(item.id);
    }

    if (action.id === "resume_later") {
      onUpdateIntentState(item.id, "pending");
    }

    if (action.id === "send_to_device") {
      onUpdateIntentState(item.id, "sent");
    }
  }

  return (
    <>
      <article className={`item-card item-card--${item.type}`}>
        <div className="item-meta">
          <span className="item-type-chip">{isBundle ? t("conversationBundle") : formatItemType(item.type)}</span>
          <span>{formatDateTime(item.updated_at)}</span>
        </div>
        <div className="item-content">
          <strong className="item-title">{label}</strong>
          <div className="smart-drop-row">
            <div className="smart-drop-main">
              <span className="smart-drop-label">{semantic?.primary_label ?? t("smartDrop")}</span>
              <span className={`intent-chip intent-chip--${intentState}`}>
                {formatIntentState(intentState, t)}
              </span>
            </div>
            <div className="smart-drop-context">
              <span>{formatSourceContext(sourceContext, t)}</span>
              {(semantic?.tags ?? []).slice(0, 4).map((tag) => (
                <span key={tag} className="smart-tag">
                  {tag}
                </span>
              ))}
            </div>
            {semantic?.summary ? <p className="smart-drop-summary">{semantic.summary}</p> : null}
            {smartActions.length ? (
              <div className="smart-suggestions" aria-label={t("smartSuggestions")}>
                {smartActions.map((action) => (
                  <button
                    key={action.id}
                    type="button"
                    className="smart-suggestion"
                    disabled={!(action.enabled || (action.id === "send_to_device" && canSendToDevice))}
                    title={!(action.enabled || (action.id === "send_to_device" && canSendToDevice)) ? t("smartActionUnavailable") : undefined}
                    onClick={() => handleSmartAction(action)}
                  >
                    {formatSuggestedAction(action, t)}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          {isText ? (
            <div className="item-text-scroll">
              <p>{textBody}</p>
            </div>
          ) : null}
          {isImage ? (
            <img
              src={convertFileSrc(item.content_ref, "asset")}
              alt={item.name ?? t("droppedImageAlt")}
              loading="lazy"
            />
          ) : null}
          {isBundle ? (
            <div className="bundle-tile">
              <strong>{item.name ?? t("conversationBundle")}</strong>
              <span>{item.text_preview ?? t("bundleTileHint")}</span>
            </div>
          ) : null}
          {!isText && !isImage && !isBundle ? (
            <div className="file-tile">
              <strong>{item.name ?? t("fileFallback")}</strong>
              <span>{item.mime_type ?? "application/octet-stream"}</span>
              <span>{formatBytes(item.size_bytes ?? 0)}</span>
            </div>
          ) : null}
        </div>
        <div className="item-actions">
          <div className="item-action-group">
            {isText ? (
              <button type="button" className="ghost" onClick={() => onCopyText(item.id)}>
                {t("copy")}
              </button>
            ) : null}
            {isBundle ? (
              <button type="button" className="ghost" onClick={() => setIsBundleOpen(true)}>
                {t("viewBundle")}
              </button>
            ) : null}
            <button type="button" className="ghost" onClick={() => void handleDownload()}>
              {t("download")}
            </button>
            {intentState !== "completed" && intentState !== "revoked" ? (
              <button type="button" className="ghost" onClick={() => onUpdateIntentState(item.id, "completed")}>
                {t("markDone")}
              </button>
            ) : null}
            {intentState !== "revoked" ? (
              <button type="button" className="ghost destructive" onClick={() => onUpdateIntentState(item.id, "revoked")}>
                {t("revoke")}
              </button>
            ) : null}
            <button type="button" className="ghost destructive" onClick={() => onDeleteItem(item.id)}>
              {t("delete")}
            </button>
          </div>
        </div>
      </article>
      {isBundleOpen ? <ConversationBundleModal item={item} onClose={() => setIsBundleOpen(false)} /> : null}
    </>
  );
}

function formatSourceContext(sourceContext: SourceContext | null | undefined, t: Translate) {
  if (!sourceContext) {
    return t("sourceUnknown");
  }

  const source = (() => {
    switch (sourceContext.source_kind) {
      case "browser_share":
        return t("sourceBrowser");
      case "drag_drop":
        return t("sourceDragDrop");
      case "file_picker":
        return t("sourceFilePicker");
      case "paste":
        return t("sourcePaste");
      case "relay":
        return t("sourceRelay");
      case "direct":
        return t("sourceDirect");
      case "composer":
      default:
        return t("sourceComposer");
    }
  })();

  return sourceContext.source_app ? `${source} · ${sourceContext.source_app}` : source;
}

function formatIntentState(intentState: IntentState, t: Translate) {
  switch (intentState) {
    case "pending":
      return t("intentPending");
    case "sent":
      return t("intentSent");
    case "resumed":
      return t("intentResumed");
    case "completed":
      return t("intentCompleted");
    case "revoked":
      return t("intentRevoked");
    case "captured":
    default:
      return t("intentCaptured");
  }
}

function formatSuggestedAction(action: SuggestedAction, t: Translate) {
  switch (action.id) {
    case "open":
      return t("openItem");
    case "resume_later":
      return t("resumeLater");
    case "send_to_device":
      return t("sendToDevice");
    case "summarize_later":
      return t("summarizeLater");
    default:
      return action.label;
  }
}
