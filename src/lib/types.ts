export type ItemType = "text" | "image" | "file";

export type Item = {
  id: string;
  type: ItemType;
  content_ref: string;
  created_at: string;
  updated_at: string;
  device_id: string;
  name?: string | null;
  mime_type?: string | null;
  size_bytes?: number | null;
  sha256?: string | null;
  text_preview?: string | null;
};

export type SyncStatus = {
  device_id: string;
  paired_devices: number;
  transport: "offline" | "lan" | "relay";
  relay_connected: boolean;
  pending_entries: number;
  pairing_token: string;
};

export type BootstrapPayload = {
  items: Item[];
  sync_status: SyncStatus;
};

export type DropTextPayload = {
  text: string;
};

export type ImportPathPayload = {
  paths: string[];
};

