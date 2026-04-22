import { invoke } from "@tauri-apps/api/core";
import type { BootstrapPayload, Item } from "./types";

export async function bootstrapApp(): Promise<BootstrapPayload> {
  return invoke("bootstrap_app");
}

export async function importText(text: string): Promise<Item> {
  return invoke("import_text", { payload: { text } });
}

export async function importPaths(paths: string[]): Promise<Item[]> {
  return invoke("import_paths", { payload: { paths } });
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

export async function exportItem(itemId: string, destinationPath: string): Promise<void> {
  return invoke("export_item", { itemId, destinationPath });
}
