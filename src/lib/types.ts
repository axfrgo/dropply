export type ItemType = "text" | "image" | "file";

export type IntentState = "captured" | "pending" | "sent" | "resumed" | "completed" | "revoked";

export type SourceKind =
  | "composer"
  | "paste"
  | "drag_drop"
  | "file_picker"
  | "browser_share"
  | "relay"
  | "direct";

export type SourceContext = {
  source_kind: SourceKind;
  source_app?: string | null;
  source_url?: string | null;
  source_title?: string | null;
  source_device_id: string;
  captured_at: string;
};

export type SemanticContext = {
  primary_label: string;
  summary?: string | null;
  extracted_text_preview?: string | null;
  tags: string[];
};

export type SuggestedActionId =
  | "copy"
  | "open"
  | "download"
  | "open_bundle"
  | "send_to_device"
  | "resume_later"
  | "summarize_later";

export type SuggestedAction = {
  id: SuggestedActionId;
  label: string;
  priority: number;
  enabled: boolean;
};

export type TrustProvenance = "local" | "paired_device" | "browser_extension";

export type TrustContext = {
  local_first: true;
  provenance: TrustProvenance;
  expires_at?: string | null;
  revoked_at?: string | null;
};

export type Item = {
  id: string;
  type: ItemType;
  content_ref: string;
  storage_path?: string | null;
  created_at: string;
  updated_at: string;
  device_id: string;
  name?: string | null;
  mime_type?: string | null;
  size_bytes?: number | null;
  sha256?: string | null;
  text_preview?: string | null;
  text_content?: string | null;
  source_context?: SourceContext | null;
  semantic_context?: SemanticContext | null;
  suggested_actions?: SuggestedAction[];
  intent_state?: IntentState;
  trust_context?: TrustContext | null;
};

export type SyncStatus = {
  device_id: string;
  paired_devices: number;
  transport: "offline" | "lan" | "relay" | "p2p local" | "direct";
  relay_connected: boolean;
  pending_entries: number;
  pairing_token: string;
};

export type BootstrapPayload = {
  items: Item[];
  sync_status: SyncStatus;
};

export type ZenithMetadata = {
  enabled: boolean;
  eligible: boolean;
  bypassed: boolean;
  entropy?: number | null;
  equation_weight_bytes?: number | null;
  verification?: string | null;
};

export type RelayItem = {
  id: string;
  type: ItemType;
  name?: string | null;
  mime_type?: string | null;
  size_bytes?: number | null;
  sha256?: string | null;
  updated_at: string;
  device_id: string;
  text_content?: string | null;
  bytes_b64?: string | null;
  deleted?: boolean | null;
  zenith_equation?: any;
  zenith_metadata?: ZenithMetadata | null;
  source_context?: SourceContext | null;
  semantic_context?: SemanticContext | null;
  suggested_actions?: SuggestedAction[];
  intent_state?: IntentState;
  trust_context?: TrustContext | null;
};

export type RelayBlob = {
  item_id: string;
  mime_type?: string | null;
  size_bytes: number;
  sha256?: string | null;
  updated_at: string;
  chunks: string[];
};

export type DropTextPayload = {
  text: string;
  source_kind?: SourceKind;
};

export type ImportPathPayload = {
  paths: string[];
  source_kind?: SourceKind;
};

export type ConversationBundleEntryRole = "reference" | "attachment";

export type ConversationBundleEntry = {
  path: string;
  role: ConversationBundleEntryRole;
  name: string;
  mime_type?: string | null;
  size_bytes: number;
  sha256: string;
};

export type ConversationBundleManifest = {
  bundle_version: string;
  title: string;
  source_label?: string | null;
  source_url?: string | null;
  created_at: string;
  transcript_path: string;
  transcript_sha256: string;
  entries: ConversationBundleEntry[];
};

export type ConversationBundleDetails = {
  manifest: ConversationBundleManifest;
  transcript_markdown: string;
};

export type ConversationBundleTextEntry = {
  path: string;
  mime_type?: string | null;
  content: string;
};

export type CreateConversationBundleInput = {
  title?: string;
  transcript_markdown: string;
  source_label?: string;
  source_url?: string;
  files: string[];
  attachments: string[];
};
