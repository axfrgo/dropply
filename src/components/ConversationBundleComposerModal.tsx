import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import { useI18n } from "../lib/i18n";
import type { CreateConversationBundleInput } from "../lib/types";

type ConversationBundleComposerModalProps = {
  isBusy: boolean;
  onClose: () => void;
  onCreateBundle: (payload: CreateConversationBundleInput) => Promise<void>;
};

type BundleFileSelection = {
  path: string;
  name: string;
};

export function ConversationBundleComposerModal({
  isBusy,
  onClose,
  onCreateBundle,
}: ConversationBundleComposerModalProps) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [sourceLabel, setSourceLabel] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [transcriptMarkdown, setTranscriptMarkdown] = useState("");
  const [files, setFiles] = useState<BundleFileSelection[]>([]);
  const [attachments, setAttachments] = useState<BundleFileSelection[]>([]);
  const [formError, setFormError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const isWorking = isBusy || isSubmitting;
  const transcriptLineCount = useMemo(
    () => transcriptMarkdown.split(/\r?\n/).filter((line) => line.trim().length > 0).length,
    [transcriptMarkdown],
  );

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !isWorking) {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isWorking, onClose]);

  function normalizeSelection(selection: string | string[] | null) {
    if (!selection) {
      return [];
    }
    return Array.isArray(selection) ? selection : [selection];
  }

  function displayName(path: string) {
    return path.split(/[/\\]/).pop() ?? path;
  }

  function mergePaths(
    current: BundleFileSelection[],
    selectedPaths: string[],
  ): BundleFileSelection[] {
    const seen = new Set(current.map((entry) => entry.path));
    const next = [...current];

    for (const path of selectedPaths) {
      if (seen.has(path)) {
        continue;
      }
      seen.add(path);
      next.push({ path, name: displayName(path) });
    }

    return next;
  }

  async function pickTranscriptFile() {
    try {
      const selection = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Markdown or text",
            extensions: ["md", "txt"],
          },
        ],
      });
      const [path] = normalizeSelection(selection);
      if (!path) {
        return;
      }

      const nextTranscript = await readTextFile(path);
      setTranscriptMarkdown(nextTranscript);
      setFormError(null);

      if (!title.trim()) {
        const name = displayName(path).replace(/\.[^.]+$/, "");
        setTitle(name);
      }
    } catch (error: unknown) {
      setFormError(error instanceof Error ? error.message : t("bundleTranscriptLoadFailed"));
    }
  }

  async function pickFiles(kind: "files" | "attachments") {
    try {
      const selection = await open({
        multiple: true,
        directory: false,
      });
      const paths = normalizeSelection(selection);
      if (!paths.length) {
        return;
      }

      if (kind === "files") {
        setFiles((current) => mergePaths(current, paths));
      } else {
        setAttachments((current) => mergePaths(current, paths));
      }
      setFormError(null);
    } catch (error: unknown) {
      setFormError(error instanceof Error ? error.message : t("bundleFilePickFailed"));
    }
  }

  function removeSelection(kind: "files" | "attachments", path: string) {
    if (kind === "files") {
      setFiles((current) => current.filter((entry) => entry.path !== path));
      return;
    }
    setAttachments((current) => current.filter((entry) => entry.path !== path));
  }

  async function handleSubmit() {
    const transcript = transcriptMarkdown.trim();
    if (!transcript) {
      setFormError(t("bundleTranscriptRequired"));
      return;
    }

    setIsSubmitting(true);
    setFormError(null);
    try {
      await onCreateBundle({
        title: title.trim() || undefined,
        transcript_markdown: transcript,
        source_label: sourceLabel.trim() || undefined,
        source_url: sourceUrl.trim() || undefined,
        files: files.map((entry) => entry.path),
        attachments: attachments.map((entry) => entry.path),
      });
      onClose();
    } catch (error: unknown) {
      setFormError(error instanceof Error ? error.message : t("bundleCreateFailed"));
    } finally {
      setIsSubmitting(false);
    }
  }

  function renderSelectionList(kind: "files" | "attachments", values: BundleFileSelection[]) {
    if (!values.length) {
      return <div className="bundle-compose-empty">{t(kind === "files" ? "bundleNoFiles" : "bundleNoAttachments")}</div>;
    }

    return (
      <div className="bundle-compose-list">
        {values.map((entry) => (
          <div key={entry.path} className="bundle-compose-chip">
            <div className="bundle-compose-chip-copy">
              <strong>{entry.name}</strong>
              <span>{entry.path}</span>
            </div>
            <button
              type="button"
              className="ghost"
              disabled={isWorking}
              onClick={() => removeSelection(kind, entry.path)}
            >
              {t("remove")}
            </button>
          </div>
        ))}
      </div>
    );
  }

  function renderRejection(reason: string) {
    return (
      <div className="bundle-compose-rejection" role="alert">
        <strong>{t("bundleRejectedTitle")}</strong>
        <p>{t("bundleRejectedHint")}</p>
        <span>{t("bundleRejectedReasonLabel")}</span>
        <code>{reason}</code>
      </div>
    );
  }

  return (
    <div className="bundle-compose-backdrop" role="presentation" onClick={() => !isWorking && onClose()}>
      <section className="bundle-compose-modal" onClick={(event) => event.stopPropagation()}>
        <div className="bundle-compose-header">
          <div>
            <p className="eyebrow">{t("conversationBundle")}</p>
            <h2>{t("createBundle")}</h2>
            <p className="bundle-compose-copy">{t("bundleComposeHint")}</p>
          </div>
          <button type="button" className="composer-tool" onClick={onClose} disabled={isWorking}>
            {t("close")}
          </button>
        </div>

        <div className="bundle-compose-grid">
          <label className="bundle-compose-field">
            <span>{t("bundleTitleLabel")}</span>
            <input
              type="text"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder={t("bundleTitlePlaceholder")}
              disabled={isWorking}
            />
          </label>

          <label className="bundle-compose-field">
            <span>{t("bundleSourceLabel")}</span>
            <input
              type="text"
              value={sourceLabel}
              onChange={(event) => setSourceLabel(event.target.value)}
              placeholder={t("bundleSourcePlaceholder")}
              disabled={isWorking}
            />
          </label>

          <label className="bundle-compose-field bundle-compose-field--full">
            <span>{t("bundleSourceUrlLabel")}</span>
            <input
              type="url"
              value={sourceUrl}
              onChange={(event) => setSourceUrl(event.target.value)}
              placeholder={t("bundleSourceUrlPlaceholder")}
              disabled={isWorking}
            />
          </label>
        </div>

        <div className="bundle-compose-section">
          <div className="bundle-compose-section-head">
            <div>
              <strong>{t("bundleTranscript")}</strong>
              <span>{t("bundleTranscriptEditorHint", { count: transcriptLineCount })}</span>
            </div>
            <button type="button" className="composer-tool" onClick={() => void pickTranscriptFile()} disabled={isWorking}>
              {t("bundleLoadTranscript")}
            </button>
          </div>
          <textarea
            className="bundle-compose-textarea"
            value={transcriptMarkdown}
            onChange={(event) => setTranscriptMarkdown(event.target.value)}
            placeholder={t("bundleTranscriptPlaceholder")}
            disabled={isWorking}
          />
        </div>

        <div className="bundle-compose-columns">
          <div className="bundle-compose-section">
            <div className="bundle-compose-section-head">
              <div>
                <strong>{t("bundleFiles")}</strong>
                <span>{t("bundleFilesHint")}</span>
              </div>
              <button type="button" className="composer-tool" onClick={() => void pickFiles("files")} disabled={isWorking}>
                {t("bundleAddFiles")}
              </button>
            </div>
            {renderSelectionList("files", files)}
          </div>

          <div className="bundle-compose-section">
            <div className="bundle-compose-section-head">
              <div>
                <strong>{t("bundleAttachments")}</strong>
                <span>{t("bundleAttachmentsHint")}</span>
              </div>
              <button
                type="button"
                className="composer-tool"
                onClick={() => void pickFiles("attachments")}
                disabled={isWorking}
              >
                {t("bundleAddAttachments")}
              </button>
            </div>
            {renderSelectionList("attachments", attachments)}
          </div>
        </div>

        {formError ? <div className="bundle-compose-error">{renderRejection(formError)}</div> : null}

        <div className="bundle-compose-actions">
          <button type="button" className="composer-tool" onClick={onClose} disabled={isWorking}>
            {t("cancel")}
          </button>
          <button
            type="button"
            className="composer-send"
            onClick={() => void handleSubmit()}
            disabled={isWorking || !transcriptMarkdown.trim()}
          >
            {isWorking ? t("bundleSaving") : t("bundleSend")}
          </button>
        </div>
      </section>
    </div>
  );
}
