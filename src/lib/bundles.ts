import type { ConversationBundleEntry, Item } from "./types";

export const CONVERSATION_BUNDLE_MIME_TYPE = "application/vnd.dropply.conversation-bundle+zip";

const PREVIEWABLE_TEXT_EXTENSIONS = new Set([
  "md",
  "txt",
  "json",
  "ts",
  "tsx",
  "js",
  "jsx",
  "rs",
  "toml",
  "yml",
  "yaml",
  "css",
  "html",
  "xml",
  "csv",
]);

export function isConversationBundleItem(item: Pick<Item, "mime_type" | "name">) {
  return (
    item.mime_type === CONVERSATION_BUNDLE_MIME_TYPE ||
    item.name?.toLowerCase().endsWith(".dropplybundle") === true
  );
}

export function isPreviewableBundleEntry(entry: ConversationBundleEntry) {
  if (entry.mime_type?.startsWith("text/")) {
    return true;
  }
  if (entry.mime_type === "application/json") {
    return true;
  }

  const extension = entry.path.split(".").pop()?.toLowerCase();
  return extension ? PREVIEWABLE_TEXT_EXTENSIONS.has(extension) : false;
}
