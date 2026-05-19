import { invoke } from "@tauri-apps/api/core";
import type {
  BootstrapPayload,
  ConversationBundleDetails,
  ConversationBundleTextEntry,
  CreateConversationBundleInput,
  IntentState,
  Item,
  RelayBlob,
  RelayItem,
  SourceKind,
} from "./types";

export async function bootstrapApp(): Promise<BootstrapPayload> {
  return invoke("bootstrap_app");
}

export async function importText(text: string, id?: string, sourceKind: SourceKind = "composer"): Promise<Item> {
  return invoke("import_text", { payload: { text, id, source_kind: sourceKind } });
}

export async function importPaths(paths: string[], sourceKind: SourceKind = "file_picker"): Promise<Item[]> {
  return invoke("import_paths", { payload: { paths, source_kind: sourceKind } });
}

export async function importConversationBundle(payload: CreateConversationBundleInput): Promise<Item> {
  return invoke("import_conversation_bundle", {
    payload: {
      title: payload.title,
      transcript_markdown: payload.transcript_markdown,
      source_label: payload.source_label,
      source_url: payload.source_url,
      files: payload.files.map((path) => ({ path })),
      attachments: payload.attachments.map((path) => ({ path })),
    },
  });
}

export async function refreshItems(): Promise<Item[]> {
  return invoke("list_items");
}

export async function copyItemText(itemId: string): Promise<void> {
  return invoke("copy_item_text", { itemId });
}

export async function deleteItem(itemId: string): Promise<void> {
  return invoke("delete_item", { itemId });
}

export async function updateItemIntentState(itemId: string, intentState: IntentState): Promise<Item | null> {
  return invoke("update_item_intent_state", { itemId, intentState });
}

export async function exportItem(itemId: string, destinationPath: string): Promise<void> {
  return invoke("export_item", { itemId, destinationPath });
}

export async function exportItemToDownloads(itemId: string): Promise<string> {
  return invoke("export_item_to_downloads", { itemId });
}

export async function openItem(itemId: string): Promise<void> {
  return invoke("open_item", { itemId });
}

export async function inspectConversationBundle(itemId: string): Promise<ConversationBundleDetails> {
  return invoke("inspect_conversation_bundle", { itemId });
}

export async function readConversationBundleEntry(
  itemId: string,
  entryPath: string,
): Promise<ConversationBundleTextEntry> {
  return invoke("read_conversation_bundle_entry", { itemId, entryPath });
}

export async function exportRelayItems(): Promise<RelayItem[]> {
  return invoke("export_relay_items");
}

export async function exportPairManifest(): Promise<RelayItem[]> {
  return invoke("export_pair_manifest");
}

export async function exportRelayBlob(itemId: string, chunkBytes: number): Promise<RelayBlob> {
  return invoke("export_relay_blob", { itemId, chunkBytes });
}

export async function setPairingToken(token: string): Promise<void> {
  return invoke("set_pairing_token", { token });
}

export async function importRelayItem(payload: RelayItem): Promise<Item> {
  return invoke("import_relay_item", { payload });
}

export async function importStagedTransfer(payload: RelayItem, stagedPath: string): Promise<Item> {
  return invoke("import_staged_transfer", { payload, stagedPath });
}

export async function resetPairingToken(): Promise<string> {
  return await invoke("reset_pairing_token");
}

export async function unpairDevice(): Promise<void> {
  return await invoke("unpair_device");
}
