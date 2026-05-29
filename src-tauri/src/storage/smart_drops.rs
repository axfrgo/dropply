use chrono::{DateTime, Utc};

use crate::models::{
    IntentState, Item, ItemType, SemanticContextPayload, SourceContextPayload, SourceKind,
    SuggestedActionId, SuggestedActionPayload, TrustContextPayload, TrustProvenance,
};
use crate::storage::bundles::{is_conversation_bundle_item, CONVERSATION_BUNDLE_MIME_TYPE};

#[derive(Clone, Debug)]
pub struct SmartDropSeed {
    pub source_kind: SourceKind,
    pub provenance: TrustProvenance,
    pub source_app: Option<String>,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
}

impl SmartDropSeed {
    pub fn local(source_kind: SourceKind) -> Self {
        Self {
            source_kind,
            provenance: TrustProvenance::Local,
            source_app: None,
            source_url: None,
            source_title: None,
        }
    }

    pub fn paired(source_kind: SourceKind) -> Self {
        Self {
            source_kind,
            provenance: TrustProvenance::PairedDevice,
            source_app: None,
            source_url: None,
            source_title: None,
        }
    }
}

pub fn apply_new_item_metadata(item: &mut Item, seed: SmartDropSeed) {
    item.intent_state = IntentState::Captured;
    item.source_context = Some(build_source_context(item, &seed, item.created_at));
    item.trust_context = Some(build_trust_context(seed.provenance, None));
    refresh_semantics(item);
}

pub fn ensure_item_metadata(item: &mut Item, fallback: SmartDropSeed) {
    if item.source_context.is_none() {
        item.source_context = Some(build_source_context(item, &fallback, item.created_at));
    }

    if item.trust_context.is_none() || fallback.provenance != TrustProvenance::Local {
        let mut trust = item
            .trust_context
            .clone()
            .unwrap_or_else(|| build_trust_context(fallback.provenance, None));
        trust.local_first = true;
        trust.provenance = fallback.provenance;
        item.trust_context = Some(trust);
    }

    refresh_semantics(item);
}

pub fn apply_intent_state(item: &mut Item, intent_state: IntentState, updated_at: DateTime<Utc>) {
    item.intent_state = intent_state;
    item.updated_at = updated_at;

    if matches!(intent_state, IntentState::Revoked) {
        let mut trust = item
            .trust_context
            .clone()
            .unwrap_or_else(|| build_trust_context(TrustProvenance::Local, None));
        trust.revoked_at = Some(updated_at);
        item.trust_context = Some(trust);
    }

    refresh_semantics(item);
}

fn build_source_context(
    item: &Item,
    seed: &SmartDropSeed,
    captured_at: DateTime<Utc>,
) -> SourceContextPayload {
    SourceContextPayload {
        source_kind: seed.source_kind,
        source_app: seed.source_app.clone(),
        source_url: seed.source_url.clone(),
        source_title: seed.source_title.clone(),
        source_device_id: item.device_id.clone(),
        captured_at,
    }
}

fn build_trust_context(
    provenance: TrustProvenance,
    revoked_at: Option<DateTime<Utc>>,
) -> TrustContextPayload {
    TrustContextPayload {
        local_first: true,
        provenance,
        expires_at: None,
        revoked_at,
    }
}

fn refresh_semantics(item: &mut Item) {
    let semantic_context = classify_item(item);
    item.suggested_actions = suggest_actions(item, &semantic_context);
    item.semantic_context = Some(semantic_context);
}

fn classify_item(item: &Item) -> SemanticContextPayload {
    let source = item.source_context.as_ref();
    let source_app = source.and_then(|context| context.source_app.as_deref());
    let source_title = source.and_then(|context| context.source_title.as_deref());
    let text_preview = item.text_preview.as_deref().map(compact_text);
    let mime = item.mime_type.as_deref().unwrap_or_default();
    let name = item.name.as_deref().unwrap_or_default();
    let lower_name = name.to_ascii_lowercase();
    let mut tags = Vec::new();

    push_tag(&mut tags, match item.item_type {
        ItemType::Text => "text",
        ItemType::Image => "image",
        ItemType::File => "file",
    });

    if let Some(context) = source {
        push_tag(&mut tags, source_kind_tag(context.source_kind));
    }

    if source_app.is_some() {
        push_tag(&mut tags, "source");
    }

    if item.size_bytes.unwrap_or_default() > 100 * 1024 * 1024 {
        push_tag(&mut tags, "large");
    }

    let primary_label = if is_conversation_bundle_item(item) {
        push_tag(&mut tags, "bundle");
        source_app
            .map(|app| format!("{app} bundle"))
            .unwrap_or_else(|| "Conversation bundle".to_string())
    } else if matches!(item.item_type, ItemType::Text) {
        if text_preview
            .as_deref()
            .map(|preview| preview.contains("http://") || preview.contains("https://"))
            .unwrap_or(false)
        {
            push_tag(&mut tags, "link");
            "Link note".to_string()
        } else {
            "Text note".to_string()
        }
    } else if matches!(item.item_type, ItemType::Image) {
        push_tag(&mut tags, "media");
        "Image".to_string()
    } else if mime == "application/pdf" || lower_name.ends_with(".pdf") {
        push_tag(&mut tags, "pdf");
        "PDF".to_string()
    } else if mime.starts_with("video/") {
        push_tag(&mut tags, "video");
        "Video".to_string()
    } else if mime.starts_with("audio/") {
        push_tag(&mut tags, "audio");
        "Audio".to_string()
    } else if mime == CONVERSATION_BUNDLE_MIME_TYPE {
        push_tag(&mut tags, "bundle");
        "Conversation bundle".to_string()
    } else if is_archive_name(&lower_name) {
        push_tag(&mut tags, "archive");
        "Archive".to_string()
    } else {
        "File".to_string()
    };

    let summary = source_title
        .map(compact_text)
        .or_else(|| text_preview.clone())
        .or_else(|| {
            source_app.map(|app| {
                source
                    .and_then(|context| context.source_url.as_deref())
                    .map(|url| format!("Captured from {app}: {url}"))
                    .unwrap_or_else(|| format!("Captured from {app}"))
            })
        });

    SemanticContextPayload {
        primary_label,
        summary,
        extracted_text_preview: text_preview,
        tags,
    }
}

fn suggest_actions(item: &Item, semantic: &SemanticContextPayload) -> Vec<SuggestedActionPayload> {
    let mut actions = Vec::new();

    if matches!(item.item_type, ItemType::Text) {
        actions.push(action(SuggestedActionId::Copy, "Copy", 10, true));
    } else {
        actions.push(action(SuggestedActionId::Open, "Open", 10, true));
    }

    if is_conversation_bundle_item(item) {
        actions.push(action(SuggestedActionId::OpenBundle, "Open bundle", 12, true));
    }

    actions.push(action(SuggestedActionId::Download, "Download", 20, true));

    if !matches!(item.intent_state, IntentState::Completed | IntentState::Revoked) {
        actions.push(action(
            SuggestedActionId::ResumeLater,
            "Resume later",
            30,
            true,
        ));
    }

    if semantic.tags.iter().any(|tag| tag == "text" || tag == "pdf" || tag == "bundle") {
        actions.push(action(
            SuggestedActionId::SummarizeLater,
            "Summarize later",
            50,
            false,
        ));
    }

    actions.push(action(
        SuggestedActionId::SendToDevice,
        "Send to device",
        60,
        false,
    ));

    actions.sort_by_key(|entry| entry.priority);
    actions
}

fn action(
    id: SuggestedActionId,
    label: impl Into<String>,
    priority: i64,
    enabled: bool,
) -> SuggestedActionPayload {
    SuggestedActionPayload {
        id,
        label: label.into(),
        priority,
        enabled,
    }
}

fn source_kind_tag(source_kind: SourceKind) -> &'static str {
    match source_kind {
        SourceKind::Composer => "composer",
        SourceKind::Paste => "paste",
        SourceKind::DragDrop => "drag",
        SourceKind::FilePicker => "picker",
        SourceKind::BrowserShare => "browser",
        SourceKind::Relay => "relay",
        SourceKind::Direct => "direct",
    }
}

fn push_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_string());
    }
}

fn compact_text(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(180).collect()
}

fn is_archive_name(lower_name: &str) -> bool {
    lower_name.ends_with(".zip")
        || lower_name.ends_with(".7z")
        || lower_name.ends_with(".rar")
        || lower_name.ends_with(".tar")
        || lower_name.ends_with(".gz")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn base_item(item_type: ItemType, name: Option<&str>, mime_type: Option<&str>) -> Item {
        Item {
            id: "item-1".to_string(),
            item_type,
            content_ref: "blobs/item".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            device_id: "device-1".to_string(),
            name: name.map(str::to_string),
            mime_type: mime_type.map(str::to_string),
            size_bytes: Some(42),
            sha256: None,
            text_preview: None,
            source_context: None,
            semantic_context: None,
            suggested_actions: Vec::new(),
            intent_state: IntentState::Captured,
            trust_context: None,
        }
    }

    #[test]
    fn classifies_pdf_file() {
        let mut item = base_item(ItemType::File, Some("brief.pdf"), Some("application/pdf"));
        apply_new_item_metadata(&mut item, SmartDropSeed::local(SourceKind::FilePicker));

        let semantic = item.semantic_context.expect("semantic context");
        assert_eq!(semantic.primary_label, "PDF");
        assert!(semantic.tags.iter().any(|tag| tag == "pdf"));
        assert!(item
            .suggested_actions
            .iter()
            .any(|action| matches!(action.id, SuggestedActionId::SummarizeLater)));
    }

    #[test]
    fn classifies_browser_bundle_with_source() {
        let mut item = base_item(
            ItemType::File,
            Some("Browser research.dropplybundle"),
            Some(CONVERSATION_BUNDLE_MIME_TYPE),
        );
        apply_new_item_metadata(
            &mut item,
            SmartDropSeed {
                source_kind: SourceKind::BrowserShare,
                provenance: TrustProvenance::BrowserExtension,
                source_app: Some("Browser".to_string()),
                source_url: Some("https://example.com/research".to_string()),
                source_title: Some("Launch review".to_string()),
            },
        );

        let semantic = item.semantic_context.expect("semantic context");
        assert_eq!(semantic.primary_label, "Browser bundle");
        assert!(semantic.tags.iter().any(|tag| tag == "browser"));
        assert!(item
            .suggested_actions
            .iter()
            .any(|action| matches!(action.id, SuggestedActionId::OpenBundle)));
    }
}
