import { useEffect, useMemo, useState } from "react";
import { inspectConversationBundle, readConversationBundleEntry } from "../lib/api";
import { isPreviewableBundleEntry } from "../lib/bundles";
import { useI18n } from "../lib/i18n";
import type {
  ConversationBundleDetails,
  ConversationBundleEntry,
  ConversationBundleTextEntry,
  Item,
} from "../lib/types";

type ConversationBundleModalProps = {
  item: Item;
  onClose: () => void;
};

type BundleSelection = { kind: "transcript" } | { kind: "entry"; path: string };

export function ConversationBundleModal({ item, onClose }: ConversationBundleModalProps) {
  const { formatBytes, formatDateTime, t } = useI18n();
  const [details, setDetails] = useState<ConversationBundleDetails | null>(null);
  const [selection, setSelection] = useState<BundleSelection>({ kind: "transcript" });
  const [bundleError, setBundleError] = useState<string | null>(null);
  const [entryError, setEntryError] = useState<string | null>(null);
  const [isLoadingBundle, setIsLoadingBundle] = useState(true);
  const [isLoadingEntry, setIsLoadingEntry] = useState(false);
  const [entryCache, setEntryCache] = useState<Record<string, ConversationBundleTextEntry>>({});

  useEffect(() => {
    let isMounted = true;

    void inspectConversationBundle(item.id)
      .then((payload) => {
        if (!isMounted) {
          return;
        }
        setDetails(payload);
        setBundleError(null);
      })
      .catch((error: unknown) => {
        if (!isMounted) {
          return;
        }
        setBundleError(error instanceof Error ? error.message : t("bundleOpenFailed"));
      })
      .finally(() => {
        if (isMounted) {
          setIsLoadingBundle(false);
        }
      });

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      isMounted = false;
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [item.id, onClose, t]);

  const selectedEntry = useMemo(() => {
    if (!details || selection.kind !== "entry") {
      return null;
    }
    return details.manifest.entries.find((entry) => entry.path === selection.path) ?? null;
  }, [details, selection]);

  useEffect(() => {
    if (!details || selection.kind !== "entry" || !selectedEntry) {
      setEntryError(null);
      return;
    }
    if (!isPreviewableBundleEntry(selectedEntry) || entryCache[selectedEntry.path]) {
      setEntryError(null);
      return;
    }

    let isMounted = true;
    setIsLoadingEntry(true);
    setEntryError(null);

    void readConversationBundleEntry(item.id, selectedEntry.path)
      .then((payload) => {
        if (!isMounted) {
          return;
        }
        setEntryCache((current) => ({
          ...current,
          [payload.path]: payload,
        }));
      })
      .catch((error: unknown) => {
        if (!isMounted) {
          return;
        }
        setEntryError(error instanceof Error ? error.message : t("bundleEntryLoadFailed"));
      })
      .finally(() => {
        if (isMounted) {
          setIsLoadingEntry(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, [details, entryCache, item.id, selectedEntry, selection, t]);

  const referenceEntries = details?.manifest.entries.filter((entry) => entry.role === "reference") ?? [];
  const attachmentEntries = details?.manifest.entries.filter((entry) => entry.role === "attachment") ?? [];
  const activeTitle =
    selection.kind === "transcript"
      ? details?.manifest.title ?? item.name ?? t("conversationBundle")
      : selectedEntry?.path ?? "";
  const activeContent =
    selection.kind === "transcript"
      ? details?.transcript_markdown ?? ""
      : selectedEntry
        ? entryCache[selectedEntry.path]?.content ?? ""
        : "";
  const activePreviewable = selectedEntry ? isPreviewableBundleEntry(selectedEntry) : true;

  function renderEntryButton(entry: ConversationBundleEntry) {
    const isActive = selection.kind === "entry" && selection.path === entry.path;
    return (
      <button
        key={entry.path}
        type="button"
        className={`bundle-modal-entry ${isActive ? "is-active" : ""}`}
        onClick={() => setSelection({ kind: "entry", path: entry.path })}
      >
        <strong>{entry.name}</strong>
        <span>{entry.path}</span>
      </button>
    );
  }

  function renderRejection(reason: string) {
    return (
      <div className="bundle-modal-rejection" role="alert">
        <strong>{t("bundleRejectedTitle")}</strong>
        <p>{t("bundleRejectedHint")}</p>
        <span>{t("bundleRejectedReasonLabel")}</span>
        <code>{reason}</code>
      </div>
    );
  }

  return (
    <div className="bundle-modal-backdrop" role="presentation" onClick={onClose}>
      <section
        className="bundle-modal"
        aria-label={details?.manifest.title ?? item.name ?? t("conversationBundle")}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="bundle-modal-header">
          <div>
            <p className="eyebrow">{t("conversationBundle")}</p>
            <h2>{details?.manifest.title ?? item.name ?? t("conversationBundle")}</h2>
            <p className="bundle-modal-copy">
              {details?.manifest.source_label
                ? `${details.manifest.source_label} - ${formatDateTime(details.manifest.created_at)}`
                : formatDateTime(details?.manifest.created_at ?? item.updated_at)}
            </p>
          </div>
          <button type="button" className="composer-tool" onClick={onClose}>
            {t("close")}
          </button>
        </div>

        {details ? (
          <div className="bundle-modal-metrics">
            <span className="status-pill">{t("bundleFilesCount", { count: referenceEntries.length })}</span>
            <span className="status-pill">{t("bundleAttachmentsCount", { count: attachmentEntries.length })}</span>
            {details.manifest.source_url ? <span className="bundle-modal-url">{details.manifest.source_url}</span> : null}
          </div>
        ) : null}

        {isLoadingBundle ? (
          <div className="bundle-modal-empty">{t("bundleLoading")}</div>
        ) : bundleError ? (
          <div className="bundle-modal-empty">{renderRejection(bundleError)}</div>
        ) : details ? (
          <div className="bundle-modal-body">
            <aside className="bundle-modal-sidebar">
              <button
                type="button"
                className={`bundle-modal-entry ${selection.kind === "transcript" ? "is-active" : ""}`}
                onClick={() => setSelection({ kind: "transcript" })}
              >
                <strong>{t("bundleTranscript")}</strong>
                <span>{details.manifest.transcript_path}</span>
              </button>

              <div className="bundle-modal-section">
                <span className="bundle-modal-section-label">{t("bundleFiles")}</span>
                {referenceEntries.length ? (
                  referenceEntries.map(renderEntryButton)
                ) : (
                  <div className="bundle-modal-empty bundle-modal-empty--inline">{t("bundleNoFiles")}</div>
                )}
              </div>

              <div className="bundle-modal-section">
                <span className="bundle-modal-section-label">{t("bundleAttachments")}</span>
                {attachmentEntries.length ? (
                  attachmentEntries.map(renderEntryButton)
                ) : (
                  <div className="bundle-modal-empty bundle-modal-empty--inline">{t("bundleNoAttachments")}</div>
                )}
              </div>
            </aside>

            <div className="bundle-modal-viewer">
              <div className="bundle-modal-viewer-meta">
                <strong>{activeTitle}</strong>
                {selectedEntry ? (
                  <span>
                    {(selectedEntry.mime_type ?? "application/octet-stream") +
                      " - " +
                      formatBytes(selectedEntry.size_bytes)}
                  </span>
                ) : (
                  <span>{t("bundleTranscriptHint")}</span>
                )}
              </div>

              {selection.kind === "entry" && !activePreviewable ? (
                <div className="bundle-modal-empty">{t("bundlePreviewUnavailable")}</div>
              ) : isLoadingEntry ? (
                <div className="bundle-modal-empty">{t("bundleLoadingEntry")}</div>
              ) : entryError ? (
                <div className="bundle-modal-empty">{renderRejection(entryError)}</div>
              ) : (
                <pre className="bundle-modal-pre">{activeContent}</pre>
              )}
            </div>
          </div>
        ) : null}
      </section>
    </div>
  );
}
