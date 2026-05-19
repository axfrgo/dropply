//! Terminal UI support for the `dropply-cli tui` command.

use std::io;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use arboard::Clipboard;
use chrono::{DateTime, Local, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use dropply_lib::models::{
    ConversationBundleDetailsPayload, IntentState, ItemPayload, ItemType, LogEntry, SourceKind,
    SuggestedActionId, TrustProvenance,
};
use dropply_lib::storage::bundles::{is_conversation_bundle_name, CONVERSATION_BUNDLE_MIME_TYPE};
use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};

use super::*;

const TUI_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const TUI_EVENT_INTERVAL: Duration = Duration::from_millis(120);
const TUI_LOG_LIMIT: usize = 24;

const COLOR_PANEL: Color = Color::Rgb(18, 25, 37);
const COLOR_PANEL_MUTED: Color = Color::Rgb(24, 34, 49);
const COLOR_BRAND: Color = Color::Rgb(112, 184, 255);
const COLOR_TEXT: Color = Color::Rgb(236, 242, 250);
const COLOR_MUTED: Color = Color::Rgb(138, 156, 177);
const COLOR_SUCCESS: Color = Color::Rgb(106, 214, 156);
const COLOR_WARN: Color = Color::Rgb(255, 190, 92);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TuiTab {
    Dashboard,
    Stream,
    Devices,
    Help,
}

impl TuiTab {
    fn all() -> [Self; 4] {
        [Self::Dashboard, Self::Stream, Self::Devices, Self::Help]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Stream => "Stream",
            Self::Devices => "Devices",
            Self::Help => "Help",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Dashboard => 0,
            Self::Stream => 1,
            Self::Devices => 2,
            Self::Help => 3,
        }
    }

    fn from_index(index: usize) -> Self {
        Self::all().get(index).copied().unwrap_or(Self::Dashboard)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Filter,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusArea {
    Menu,
    Content,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FooterTone {
    Info,
    Success,
    Warn,
}

struct FooterMessage {
    text: String,
    tone: FooterTone,
}

struct OverlayState {
    title: String,
    body: String,
    scroll: u16,
}

struct ConfirmDeleteState {
    item_id: String,
    item_name: String,
}

struct ComposeTextState {
    draft: String,
}

struct ActionPaletteState {
    selection: usize,
}

enum ModalState {
    Preview(OverlayState),
    ConfirmDelete(ConfirmDeleteState),
    ComposeText(ComposeTextState),
    ActionPalette(ActionPaletteState),
}

#[derive(Clone, Copy)]
enum TuiAction {
    Refresh,
    Pull,
    ComposeText,
    CopyText,
    OpenItem,
    ExportItem,
    MarkPending,
    MarkCompleted,
    RevokeIntent,
    DeleteItem,
    OpenPairPage,
}

impl TuiAction {
    fn all() -> [Self; 11] {
        [
            Self::Refresh,
            Self::Pull,
            Self::ComposeText,
            Self::CopyText,
            Self::OpenItem,
            Self::ExportItem,
            Self::MarkPending,
            Self::MarkCompleted,
            Self::RevokeIntent,
            Self::DeleteItem,
            Self::OpenPairPage,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Refresh => "Refresh session",
            Self::Pull => "Pull from relay",
            Self::ComposeText => "Compose text note",
            Self::CopyText => "Copy selected text",
            Self::OpenItem => "Open selected item",
            Self::ExportItem => "Export selected item",
            Self::MarkPending => "Mark selected item later",
            Self::MarkCompleted => "Mark selected item done",
            Self::RevokeIntent => "Revoke selected Smart Drop",
            Self::DeleteItem => "Delete selected item",
            Self::OpenPairPage => "Open pair page",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Refresh => "Reload local cache, remote status, and recent activity.",
            Self::Pull => "Import newly shared relay items and propagate deletes.",
            Self::ComposeText => "Write a note directly from the TUI and push it into the stream.",
            Self::CopyText => "Copy the currently selected text item to the clipboard.",
            Self::OpenItem => "Open the selected item with the desktop default app.",
            Self::ExportItem => "Export the selected stream item to Downloads.",
            Self::MarkPending => "Keep the artifact and mark its intent as pending.",
            Self::MarkCompleted => "Mark the selected Smart Drop as completed.",
            Self::RevokeIntent => "Keep the row but revoke this Smart Drop's intent.",
            Self::DeleteItem => "Remove the selected item locally and publish the deletion snapshot.",
            Self::OpenPairPage => "Open the active Dropply pair page in your browser.",
        }
    }
}

struct TuiData {
    snapshot: StatusSnapshot,
    recent_logs: Vec<LogEntry>,
}

struct TuiState {
    tab: TuiTab,
    focus: FocusArea,
    stream_selection: usize,
    device_selection: usize,
    input_mode: InputMode,
    stream_filter: String,
    filter_draft: String,
    filter_before_edit: String,
    modal: Option<ModalState>,
    bundle_preview: Option<(String, ConversationBundleDetailsPayload)>,
    data: Option<TuiData>,
    footer: FooterMessage,
    last_refresh: Option<DateTime<Local>>,
    should_quit: bool,
}

pub async fn run_tui_command(runtime: &CliRuntime) -> AppResult<()> {
    if runtime.is_json() {
        return Err(anyhow!("tui is only available in pretty mode").into());
    }

    let mut state = TuiState::new();
    state.refresh(runtime).await?;

    let _terminal_guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| state.render(frame))?;

        if state.should_quit {
            break;
        }

        let timeout = TUI_EVENT_INTERVAL
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    state.handle_key(runtime, key).await?;
                }
            }
        }

        if last_tick.elapsed() >= TUI_EVENT_INTERVAL {
            last_tick = Instant::now();
        }

        let should_refresh = state
            .last_refresh
            .map(|value| {
                (Local::now() - value)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0))
                    >= TUI_REFRESH_INTERVAL
            })
            .unwrap_or(true);

        if should_refresh && state.input_mode == InputMode::Normal && state.modal.is_none() {
            state.try_refresh(runtime).await;
        }
    }

    terminal.show_cursor()?;
    Ok(())
}

impl TuiState {
    fn new() -> Self {
        Self {
            tab: TuiTab::Dashboard,
            focus: FocusArea::Menu,
            stream_selection: 0,
            device_selection: 0,
            input_mode: InputMode::Normal,
            stream_filter: String::new(),
            filter_draft: String::new(),
            filter_before_edit: String::new(),
            modal: None,
            bundle_preview: None,
            data: None,
            footer: FooterMessage {
                text: "Dropply TUI ready. Tab enters a page, Left returns to the menu.".to_string(),
                tone: FooterTone::Info,
            },
            last_refresh: None,
            should_quit: false,
        }
    }

    async fn refresh(&mut self, runtime: &CliRuntime) -> AppResult<()> {
        let snapshot = collect_status_snapshot(runtime).await?;
        let recent_logs = runtime.storage.list_recent_logs(TUI_LOG_LIMIT)?;
        self.data = Some(TuiData {
            snapshot,
            recent_logs,
        });
        self.last_refresh = Some(Local::now());
        self.clamp_selection();
        self.refresh_bundle_preview(runtime).await;
        self.set_footer("Live session refreshed.", FooterTone::Success);
        Ok(())
    }

    async fn try_refresh(&mut self, runtime: &CliRuntime) {
        if let Err(error) = self.refresh(runtime).await {
            self.set_footer(
                format!("Refresh failed: {}", shorten_line(&error.to_string(), 120)),
                FooterTone::Warn,
            );
        }
    }

    async fn handle_key(&mut self, runtime: &CliRuntime, key: KeyEvent) -> AppResult<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return Ok(());
        }

        if self.modal.is_some() {
            self.handle_modal_key(runtime, key).await?;
            return Ok(());
        }

        match self.input_mode {
            InputMode::Filter => {
                self.handle_filter_key(key);
                Ok(())
            }
            InputMode::Normal => self.handle_normal_key(runtime, key).await,
        }
    }

    async fn handle_modal_key(&mut self, runtime: &CliRuntime, key: KeyEvent) -> AppResult<()> {
        let Some(modal) = self.modal.as_mut() else {
            return Ok(());
        };

        match modal {
            ModalState::Preview(overlay) => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.modal = None,
                KeyCode::Down | KeyCode::Char('j') => overlay.scroll = overlay.scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => overlay.scroll = overlay.scroll.saturating_sub(1),
                KeyCode::PageDown => overlay.scroll = overlay.scroll.saturating_add(12),
                KeyCode::PageUp => overlay.scroll = overlay.scroll.saturating_sub(12),
                KeyCode::Home | KeyCode::Char('g') => overlay.scroll = 0,
                KeyCode::End | KeyCode::Char('G') => overlay.scroll = u16::MAX,
                _ => {}
            },
            ModalState::ConfirmDelete(confirm) => match key.code {
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.modal = None;
                    self.set_footer("Delete canceled.", FooterTone::Warn);
                }
                KeyCode::Enter | KeyCode::Char('y') => {
                    let item_id = confirm.item_id.clone();
                    let item_name = confirm.item_name.clone();
                    self.modal = None;
                    runtime.storage.delete_item(&item_id).await?;
                    let relay = sync_items_to_relay(runtime, &[]).await?;
                    self.refresh(runtime).await?;
                    self.set_footer(
                        if relay.remote_paired {
                            format!("Deleted {item_name} and published the deletion snapshot.")
                        } else {
                            format!("Deleted {item_name} locally. Pair another device to sync the delete later.")
                        },
                        FooterTone::Success,
                    );
                }
                _ => {}
            },
            ModalState::ComposeText(compose) => match key.code {
                KeyCode::Esc => {
                    self.modal = None;
                    self.set_footer("Text composer canceled.", FooterTone::Warn);
                }
                KeyCode::Backspace => {
                    compose.draft.pop();
                }
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.submit_composed_text(runtime).await?;
                }
                KeyCode::Enter => compose.draft.push('\n'),
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.submit_composed_text(runtime).await?;
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    compose.draft.push(ch);
                }
                _ => {}
            },
            ModalState::ActionPalette(palette) => match key.code {
                KeyCode::Esc => self.modal = None,
                KeyCode::Down | KeyCode::Char('j') => {
                    palette.selection = (palette.selection + 1) % TuiAction::all().len();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    palette.selection = palette
                        .selection
                        .checked_sub(1)
                        .unwrap_or(TuiAction::all().len() - 1);
                }
                KeyCode::Enter => {
                    let action = TuiAction::all()
                        .get(palette.selection)
                        .copied()
                        .unwrap_or(TuiAction::Refresh);
                    self.modal = None;
                    self.run_action(runtime, action).await?;
                }
                _ => {}
            },
        }

        Ok(())
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.stream_filter = self.filter_before_edit.clone();
                self.filter_draft = self.stream_filter.clone();
                self.input_mode = InputMode::Normal;
                self.clamp_selection();
                self.set_footer("Filter edit canceled.", FooterTone::Warn);
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::Normal;
                self.set_footer(
                    if self.stream_filter.trim().is_empty() {
                        "Filter cleared.".to_string()
                    } else {
                        format!("Stream filter applied: {}", self.stream_filter)
                    },
                    FooterTone::Success,
                );
            }
            KeyCode::Backspace => {
                self.filter_draft.pop();
                self.stream_filter = self.filter_draft.clone();
                self.stream_selection = 0;
                self.clamp_selection();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter_draft.push(ch);
                self.stream_filter = self.filter_draft.clone();
                self.stream_selection = 0;
                self.clamp_selection();
            }
            _ => {}
        }
    }

    async fn handle_normal_key(&mut self, runtime: &CliRuntime, key: KeyEvent) -> AppResult<()> {
        match key.code {
            KeyCode::Char('?') => {
                self.tab = TuiTab::Help;
                self.focus = FocusArea::Menu;
                self.set_footer("Help opened. Tab back when you're ready.", FooterTone::Info);
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    FocusArea::Menu => FocusArea::Content,
                    FocusArea::Content => FocusArea::Menu,
                };
                self.set_footer(
                    match self.focus {
                        FocusArea::Menu => "Top menu focused. Use Left/Right to switch pages, Down or Tab to re-enter.",
                        FocusArea::Content => "Page content focused. Use Left, Esc, or Tab to get back to the menu.",
                    },
                    FooterTone::Info,
                );
            }
            KeyCode::Right | KeyCode::Char('l') if self.focus == FocusArea::Menu => {
                self.tab = TuiTab::from_index((self.tab.index() + 1) % TuiTab::all().len());
            }
            KeyCode::Left | KeyCode::Char('h') if self.focus == FocusArea::Menu => {
                let index = self.tab.index().checked_sub(1).unwrap_or(TuiTab::all().len() - 1);
                self.tab = TuiTab::from_index(index);
            }
            KeyCode::Left | KeyCode::Char('h') if self.focus == FocusArea::Content => {
                self.focus = FocusArea::Menu;
                self.set_footer(
                    "Returned to the top menu. Use Left/Right to switch pages.",
                    FooterTone::Info,
                );
            }
            KeyCode::Down | KeyCode::Enter if self.focus == FocusArea::Menu => {
                self.focus = FocusArea::Content;
                self.set_footer(
                    "Page content focused. Use Left, Esc, or Tab to get back to the menu.",
                    FooterTone::Info,
                );
            }
            KeyCode::Char('1') => {
                self.tab = TuiTab::Dashboard;
                self.focus = FocusArea::Menu;
            }
            KeyCode::Char('2') => {
                self.tab = TuiTab::Stream;
                self.focus = FocusArea::Menu;
            }
            KeyCode::Char('3') => {
                self.tab = TuiTab::Devices;
                self.focus = FocusArea::Menu;
            }
            KeyCode::Char('4') => {
                self.tab = TuiTab::Help;
                self.focus = FocusArea::Menu;
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc if self.focus == FocusArea::Content => {
                self.focus = FocusArea::Menu;
                self.set_footer(
                    "Returned to the top menu. Use Left/Right to switch pages.",
                    FooterTone::Info,
                );
            }
            KeyCode::Esc => {
                self.set_footer("Top menu already focused. Press q to quit.", FooterTone::Info);
            }
            KeyCode::Char('a') | KeyCode::Char(':') => {
                self.modal = Some(ModalState::ActionPalette(ActionPaletteState { selection: 0 }));
                self.set_footer("Action palette opened.", FooterTone::Info);
            }
            KeyCode::Char('r') => {
                self.refresh(runtime).await?;
            }
            KeyCode::Char('o') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                let Some(item_id) = self.selected_item().map(|item| item.id.clone()) else {
                    return Ok(());
                };
                runtime.storage.open_item(&item_id).await?;
                self.set_footer("Opened the selected item with the desktop default app.", FooterTone::Success);
            }
            KeyCode::Char('o') => {
                let pairing = runtime.pairing()?;
                open_pair_portal(&pairing.pairing_token)?;
                self.set_footer("Opened the Dropply pair page in your browser.", FooterTone::Success);
            }
            KeyCode::Char('p') => {
                let summary = tui_pull(runtime).await?;
                self.refresh(runtime).await?;
                self.set_footer(
                    format!(
                        "Pulled {} new item(s) and removed {} deleted item(s).",
                        summary.imported_count, summary.deleted_count
                    ),
                    FooterTone::Success,
                );
            }
            KeyCode::Char('n') => {
                self.modal = Some(ModalState::ComposeText(ComposeTextState {
                    draft: String::new(),
                }));
                self.set_footer("Compose a note. Ctrl+S sends it. Esc cancels.", FooterTone::Info);
            }
            KeyCode::Char('/') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.input_mode = InputMode::Filter;
                self.filter_before_edit = self.stream_filter.clone();
                self.filter_draft = self.stream_filter.clone();
                self.set_footer("Type to filter the stream. Enter applies, Esc cancels.", FooterTone::Info);
            }
            KeyCode::Char('c') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.stream_filter.clear();
                self.filter_draft.clear();
                self.stream_selection = 0;
                self.clamp_selection();
                self.set_footer("Stream filter cleared.", FooterTone::Success);
            }
            KeyCode::Char('y') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                if let Some(item) = self.selected_item() {
                    if matches!(item.item_type, ItemType::Text) {
                        let text = runtime.storage.item_text(&item.id).await?;
                        let Some(text) = text else {
                            self.set_footer("That item no longer has local text content.", FooterTone::Warn);
                            return Ok(());
                        };
                        let mut clipboard = Clipboard::new().map_err(|err| anyhow!(err.to_string()))?;
                        clipboard
                            .set_text(text)
                            .map_err(|err| anyhow!(err.to_string()))?;
                        self.set_footer("Copied the selected text item to your clipboard.", FooterTone::Success);
                    } else {
                        self.set_footer("Copy is available for text items only right now.", FooterTone::Warn);
                    }
                }
            }
            KeyCode::Char('e') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                let Some(item_id) = self.selected_item().map(|item| item.id.clone()) else {
                    return Ok(());
                };
                let exported_path = runtime.storage.export_item_to_downloads(&item_id).await?;
                self.set_footer(
                    format!("Exported selected item to {}.", exported_path),
                    FooterTone::Success,
                );
            }
            KeyCode::Char('m') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.update_selected_intent_state(runtime, IntentState::Pending).await?;
            }
            KeyCode::Char('x') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.update_selected_intent_state(runtime, IntentState::Completed).await?;
            }
            KeyCode::Char('v') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.update_selected_intent_state(runtime, IntentState::Revoked).await?;
            }
            KeyCode::Char('d') if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                if let Some(item) = self.selected_item().cloned() {
                    let item_name = item_display_name(&item);
                    self.modal = Some(ModalState::ConfirmDelete(ConfirmDeleteState {
                        item_id: item.id,
                        item_name,
                    }));
                    self.set_footer("Confirm delete with Enter or y. Esc cancels.", FooterTone::Warn);
                }
            }
            KeyCode::Enter if self.tab == TuiTab::Stream && self.focus == FocusArea::Content => {
                self.open_item_overlay(runtime).await;
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == FocusArea::Content => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') if self.focus == FocusArea::Content => self.move_selection(-1),
            KeyCode::Home | KeyCode::Char('g') if self.focus == FocusArea::Content => self.move_selection_to_start(),
            KeyCode::End | KeyCode::Char('G') if self.focus == FocusArea::Content => self.move_selection_to_end(),
            _ => {}
        }

        if self.tab == TuiTab::Stream && self.focus == FocusArea::Content {
            self.refresh_bundle_preview(runtime).await;
        }

        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 72 || area.height < 24 {
            self.render_small_terminal(frame, area);
            return;
        }

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(12), Constraint::Length(3)])
            .split(area);

        self.render_header(frame, layout[0]);
        match self.tab {
            TuiTab::Dashboard => self.render_dashboard(frame, layout[1]),
            TuiTab::Stream => self.render_stream(frame, layout[1]),
            TuiTab::Devices => self.render_devices(frame, layout[1]),
            TuiTab::Help => self.render_help(frame, layout[1]),
        }
        self.render_footer(frame, layout[2]);

        if let Some(modal) = self.modal.as_ref() {
            self.render_modal(frame, modal);
        }
    }

    fn render_small_terminal(&self, frame: &mut Frame, area: Rect) {
        let body = [
            "Dropply TUI",
            "",
            "This view needs a little more room.",
            "Resize the terminal to at least 72x24.",
            "",
            "Tip: dropply-cli status still works well",
            "in very small terminal sessions.",
        ]
        .join("\n");
        let panel = Paragraph::new(body)
            .style(Style::default().fg(COLOR_TEXT))
            .alignment(Alignment::Center)
            .block(panel_block("Terminal too small"));
        frame.render_widget(panel, area);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let block = panel_block("Dropply TUI");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(2)])
            .split(inner);

        let snapshot = self.data.as_ref().map(|value| &value.snapshot);
        let remote_items = snapshot
            .and_then(|value| value.remote.as_ref().map(|remote| remote.item_count))
            .map(|count| count.to_string())
            .unwrap_or_else(|| "offline".to_string());
        let linked_devices = snapshot
            .and_then(|value| value.remote.as_ref().map(|remote| remote.paired_device_count.saturating_sub(1)))
            .unwrap_or(0);
        let local_items = snapshot.map(|value| value.local_items.len()).unwrap_or(0);
        let focus_label = match self.focus {
            FocusArea::Menu => "menu focus",
            FocusArea::Content => "page focus",
        };

        let summary = Line::from(vec![
            Span::styled("Drop anything. Get it everywhere.  ", Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{local_items} local"), Style::default().fg(COLOR_BRAND)),
            Span::styled("  |  ", Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("{remote_items} remote"), Style::default().fg(COLOR_BRAND)),
            Span::styled("  |  ", Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("{linked_devices} linked"), Style::default().fg(COLOR_BRAND)),
            Span::styled("  |  ", Style::default().fg(COLOR_MUTED)),
            Span::styled(focus_label, Style::default().fg(COLOR_WARN)),
        ]);
        frame.render_widget(Paragraph::new(summary), rows[0]);

        let titles = TuiTab::all()
            .into_iter()
            .map(|tab| {
                let label = match tab {
                    TuiTab::Stream => {
                        let count = self.filtered_item_indices().len();
                        format!(" {} ({count}) ", tab.title())
                    }
                    _ => format!(" {} ", tab.title()),
                };
                Line::from(label)
            })
            .collect::<Vec<_>>();
        let tabs = Tabs::new(titles)
            .select(self.tab.index())
            .style(Style::default().fg(COLOR_MUTED))
            .highlight_style(if self.focus == FocusArea::Menu {
                Style::default()
                    .fg(COLOR_TEXT)
                    .bg(COLOR_BRAND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(COLOR_BRAND)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            })
            .divider(" ");
        frame.render_widget(tabs, rows[1]);
    }

    fn render_dashboard(&self, frame: &mut Frame, area: Rect) {
        let Some(data) = self.data.as_ref() else {
            frame.render_widget(loading_panel("Loading Dropply state..."), area);
            return;
        };

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(10)])
            .split(area);

        let cards = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(layout[0]);

        let remote_items = data
            .snapshot
            .remote
            .as_ref()
            .map(|value| value.item_count.to_string())
            .unwrap_or_else(|| "offline".to_string());
        let linked_devices = data
            .snapshot
            .remote
            .as_ref()
            .map(|value| value.paired_device_count.saturating_sub(1).to_string())
            .unwrap_or_else(|| "0".to_string());
        let recent_sync = data
            .recent_logs
            .first()
            .map(|log| format_relative_time(log.updated_at))
            .unwrap_or_else(|| "quiet".to_string());

        render_metric_card(
            frame,
            cards[0],
            "Local cache",
            data.snapshot.local_items.len().to_string(),
            "Items ready on this device",
            COLOR_BRAND,
        );
        render_metric_card(
            frame,
            cards[1],
            "Remote stream",
            remote_items,
            "Relay snapshot visible to peers",
            COLOR_SUCCESS,
        );
        render_metric_card(
            frame,
            cards[2],
            "Linked devices",
            linked_devices,
            "Other devices in this Dropply session",
            COLOR_WARN,
        );
        render_metric_card(
            frame,
            cards[3],
            "Last sync activity",
            recent_sync,
            "Most recent local stream mutation",
            COLOR_TEXT,
        );

        let lower = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(layout[1]);

        self.render_activity_list(frame, lower[0], true);
        self.render_dashboard_sidecar(frame, lower[1], data);
    }

    fn render_dashboard_sidecar(&self, frame: &mut Frame, area: Rect, data: &TuiData) {
        let block = panel_block("Session pulse");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let pairing = &data.snapshot.pairing;
        let pair_url = pair_portal_url(&pairing.pairing_token);
        let remote_summary = if let Some(remote) = data.snapshot.remote.as_ref() {
            format!(
                "{} paired peers, {} item(s), {}",
                remote.paired_device_count.saturating_sub(1),
                remote.item_count,
                if remote.paired { "connected" } else { "waiting" }
            )
        } else {
            "Remote status unavailable right now.".to_string()
        };

        let body = [
            format!("Device: {}", pairing.display_name),
            format!("Pair token: {}", pairing.pairing_token),
            format!("Pair URL: {pair_url}"),
            format!("Transport: {}", data.snapshot.transport_mode),
            format!("Storage: {}", if data.snapshot.used_storage_fallback { "fallback profile" } else { "primary profile" }),
            String::new(),
            "Session summary".to_string(),
            remote_summary,
            String::new(),
            "Quick actions".to_string(),
            "o open pair page".to_string(),
            "p pull from relay".to_string(),
            "r refresh now".to_string(),
        ]
        .join("\n");

        let paragraph = Paragraph::new(body)
            .style(Style::default().fg(COLOR_TEXT))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    fn render_stream(&self, frame: &mut Frame, area: Rect) {
        let Some(data) = self.data.as_ref() else {
            frame.render_widget(loading_panel("Loading Dropply stream..."), area);
            return;
        };

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(37), Constraint::Percentage(63)])
            .split(area);

        let title = if self.input_mode == InputMode::Filter {
            format!("Stream filter: {}", self.filter_draft)
        } else if self.stream_filter.trim().is_empty() {
            if self.focus == FocusArea::Content {
                "Stream • active".to_string()
            } else {
                "Stream".to_string()
            }
        } else {
            format!("Stream filtered by '{}'", self.stream_filter)
        };

        let mut list_state = ListState::default();
        let filtered_indices = self.filtered_item_indices();
        if !filtered_indices.is_empty() {
            list_state.select(Some(self.stream_selection.min(filtered_indices.len().saturating_sub(1))));
        }

        let items = filtered_indices
            .iter()
            .map(|index| {
                let item = &data.snapshot.local_items[*index];
                let badge = item_badge(item);
                let name = item_display_name(item);
                let smart_label = item
                    .semantic_context
                    .as_ref()
                    .map(|semantic| semantic.primary_label.clone())
                    .unwrap_or_else(|| "Smart Drop".to_string());
                let subtitle = format!(
                    "{}  |  {}  |  {}  |  {}",
                    badge,
                    smart_label,
                    intent_state_label(item.intent_state),
                    format_relative_time(item.updated_at),
                );
                ListItem::new(vec![
                    Line::from(Span::styled(name, Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD))),
                    Line::from(Span::styled(subtitle, Style::default().fg(COLOR_MUTED))),
                ])
            })
            .collect::<Vec<_>>();

        let list = List::new(items)
            .block(panel_block(&title))
            .highlight_style(
                Style::default()
                    .bg(COLOR_BRAND)
                    .fg(COLOR_PANEL)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        frame.render_stateful_widget(list, columns[0], &mut list_state);

        let preview_title = self
            .selected_item()
            .map(|item| format!(
                "Preview{}: {}",
                if self.focus == FocusArea::Content { " • active" } else { "" },
                item_display_name(item)
            ))
            .unwrap_or_else(|| "Preview".to_string());
        let preview_text = self.stream_preview_text();
        let preview = Paragraph::new(preview_text)
            .block(panel_block(&preview_title))
            .style(Style::default().fg(COLOR_TEXT))
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, columns[1]);
    }

    fn render_devices(&self, frame: &mut Frame, area: Rect) {
        let Some(data) = self.data.as_ref() else {
            frame.render_widget(loading_panel("Loading Dropply devices..."), area);
            return;
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(11), Constraint::Min(10)])
            .split(area);
        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(rows[0]);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(rows[1]);

        let pairing = &data.snapshot.pairing;
        let top_left_body = [
            format!("Display name: {}", pairing.display_name),
            format!("Device ID: {}", pairing.device_id),
            format!("Pair token: {}", pairing.pairing_token),
            format!("Pair URL: {}", pair_portal_url(&pairing.pairing_token)),
            format!("Transport: {}", data.snapshot.transport_mode),
            format!("Data dir: {}", data.snapshot.data_dir),
        ]
        .join("\n");
        frame.render_widget(
            Paragraph::new(top_left_body)
                .block(panel_block("Local device"))
                .style(Style::default().fg(COLOR_TEXT))
                .wrap(Wrap { trim: false }),
            top[0],
        );

        let remote_devices = data
            .snapshot
            .remote
            .as_ref()
            .map(|value| value.devices.as_slice())
            .unwrap_or(&[]);
        let mut list_state = ListState::default();
        if !remote_devices.is_empty() {
            list_state.select(Some(self.device_selection.min(remote_devices.len().saturating_sub(1))));
        }
        let list_items = if remote_devices.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "No remote devices linked yet.",
                Style::default().fg(COLOR_MUTED),
            )))]
        } else {
            remote_devices
                .iter()
                .map(|device| {
                    let title = format!("{}  [{}]", device.label, device.device_type);
                    let detail = format!(
                        "{}  |  {}",
                        device.transport_preference,
                        format_last_seen(device.last_seen_at)
                    );
                    ListItem::new(vec![
                        Line::from(Span::styled(title, Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD))),
                        Line::from(Span::styled(detail, Style::default().fg(COLOR_MUTED))),
                    ])
                })
                .collect::<Vec<_>>()
        };
        let device_list = List::new(list_items)
            .block(panel_block(if self.focus == FocusArea::Content {
                "Linked devices • active"
            } else {
                "Linked devices"
            }))
            .highlight_style(
                Style::default()
                    .bg(COLOR_BRAND)
                    .fg(COLOR_PANEL)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        frame.render_stateful_widget(device_list, top[1], &mut list_state);

        let qr_or_hint = if bottom[0].width >= 24 {
            render_compact_pair_qr(&pair_portal_url(&pairing.pairing_token))
                .unwrap_or_else(|_| "Terminal QR unavailable right now.".to_string())
        } else {
            format!(
                "Scan from your phone or open the pair page:\n\n{}",
                pair_portal_url(&pairing.pairing_token)
            )
        };
        frame.render_widget(
            Paragraph::new(qr_or_hint)
                .block(panel_block("Pairing handoff"))
                .style(Style::default().fg(COLOR_TEXT))
                .wrap(Wrap { trim: false }),
            bottom[0],
        );

        let detail = if let Some(device) = remote_devices
            .get(self.device_selection.min(remote_devices.len().saturating_sub(1)))
        {
            [
                format!("Label: {}", device.label),
                format!("Type: {}", device.device_type),
                format!("Transport: {}", device.transport_preference),
                format!("Seen: {}", format_last_seen(device.last_seen_at)),
                String::new(),
                "Quick actions".to_string(),
                "o open pair page".to_string(),
                "p pull from relay".to_string(),
                "Tab switch panes".to_string(),
            ]
            .join("\n")
        } else {
            [
                "Open Dropply on your phone, browser, or another desktop.".to_string(),
                "Use the pair token or QR to link the same session.".to_string(),
                String::new(),
                "Once another device registers, it will appear here".to_string(),
                "with its last-seen time and transport preference.".to_string(),
            ]
            .join("\n")
        };
        frame.render_widget(
            Paragraph::new(detail)
                .block(panel_block("Selected device"))
                .style(Style::default().fg(COLOR_TEXT))
                .wrap(Wrap { trim: false }),
            bottom[1],
        );
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help = [
            "Dropply TUI",
            "",
            "Tabs",
            "1 dashboard",
            "2 stream",
            "3 devices",
            "4 help",
            "Left / Right switch pages when menu focus is active",
            "Tab toggles between the top menu and the current page",
            "Down or Enter enters the current page from the menu",
            "Left or Esc returns from a page back to the menu",
            "",
            "Global keys",
            "r refresh live data",
            "p pull new relay items",
            "n compose a new text note",
            "a or : open the action palette",
            "o open the pair page",
            "q quit",
            "",
            "Stream keys",
            "j/k or arrows move selection",
            "/ filter stream items",
            "c clear filter",
            "Enter open a full detail overlay",
            "y copy selected text item",
            "o open selected item with the default desktop app",
            "e export selected item to Downloads",
            "m mark selected Smart Drop pending",
            "x mark selected Smart Drop done",
            "v revoke selected Smart Drop intent",
            "d delete the selected item (with confirmation)",
            "",
            "Overlay keys",
            "j/k scroll",
            "Esc or Enter close",
            "",
            "Composer keys",
            "Ctrl+S or Ctrl+Enter send note",
            "Esc cancel composer",
            "",
            "Why this exists",
            "The TUI is meant to be a real power-user surface for Dropply:",
            "fast enough for SSH and terminal-heavy workflows, but still",
            "plugged into the same local cache, Smart Drops, bundles, and",
            "pair session the desktop app already uses.",
        ]
        .join("\n");

        frame.render_widget(
            Paragraph::new(help)
                .block(panel_block("Keymap and notes"))
                .style(Style::default().fg(COLOR_TEXT))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_activity_list(&self, frame: &mut Frame, area: Rect, show_title_bar: bool) {
        let Some(data) = self.data.as_ref() else {
            frame.render_widget(loading_panel("Loading activity..."), area);
            return;
        };

        let items = if data.recent_logs.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "No local activity yet. Import, send, or pull something first.",
                Style::default().fg(COLOR_MUTED),
            )))]
        } else {
            data.recent_logs
                .iter()
                .map(|entry| {
                    let title = format!(
                        "{}  {}",
                        format_relative_time(entry.updated_at),
                        summarize_log_title(entry)
                    );
                    let detail = summarize_log_detail(entry);
                    ListItem::new(vec![
                        Line::from(Span::styled(title, Style::default().fg(COLOR_TEXT))),
                        Line::from(Span::styled(detail, Style::default().fg(COLOR_MUTED))),
                    ])
                })
                .collect::<Vec<_>>()
        };

        let title = if show_title_bar {
            "Recent activity"
        } else {
            "Activity"
        };
        frame.render_widget(List::new(items).block(panel_block(title)), area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let block = panel_block("Controls");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);

        let footer_color = match self.footer.tone {
            FooterTone::Info => COLOR_BRAND,
            FooterTone::Success => COLOR_SUCCESS,
            FooterTone::Warn => COLOR_WARN,
        };

        let status_line = Line::from(vec![
            Span::styled("Status: ", Style::default().fg(COLOR_MUTED).add_modifier(Modifier::BOLD)),
            Span::styled(&self.footer.text, Style::default().fg(footer_color)),
        ]);
        frame.render_widget(Paragraph::new(status_line), rows[0]);

        let hint_text = if self.input_mode == InputMode::Filter {
            "Filter mode: type to narrow the stream, Enter applies, Esc cancels."
        } else if self.focus == FocusArea::Menu {
            "Menu focus  Left/Right switch pages  Down or Tab enter page  n note  a actions  q quit"
        } else {
            "Page focus  Left/Esc menu  j/k move  Enter inspect  o open  m later  x done  v revoke"
        };

        let last_refresh = self
            .last_refresh
            .map(|value| value.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "--:--:--".to_string());

        let hint = Line::from(vec![
            Span::styled(hint_text, Style::default().fg(COLOR_MUTED)),
            Span::styled("  |  ", Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("last refresh {last_refresh}"), Style::default().fg(COLOR_TEXT)),
        ]);
        frame.render_widget(Paragraph::new(hint), rows[1]);
    }

    fn render_modal(&self, frame: &mut Frame, modal: &ModalState) {
        match modal {
            ModalState::Preview(overlay) => {
                let popup = centered_rect(84, 82, frame.area());
                frame.render_widget(Clear, popup);
                let block = panel_block(&overlay.title);
                let inner = block.inner(popup);
                frame.render_widget(block, popup);
                let paragraph = Paragraph::new(overlay.body.as_str())
                    .style(Style::default().fg(COLOR_TEXT))
                    .wrap(Wrap { trim: false })
                    .scroll((overlay.scroll, 0));
                frame.render_widget(paragraph, inner);
            }
            ModalState::ConfirmDelete(confirm) => {
                let popup = centered_rect(52, 28, frame.area());
                frame.render_widget(Clear, popup);
                let body = [
                    "Delete this item from your Dropply cache?",
                    "",
                    &confirm.item_name,
                    "",
                    "Press Enter or y to confirm.",
                    "Press Esc or n to cancel.",
                ]
                .join("\n");
                frame.render_widget(
                    Paragraph::new(body)
                        .block(panel_block("Confirm delete"))
                        .style(Style::default().fg(COLOR_TEXT))
                        .alignment(Alignment::Left)
                        .wrap(Wrap { trim: false }),
                    popup,
                );
            }
            ModalState::ComposeText(compose) => {
                let popup = centered_rect(78, 72, frame.area());
                frame.render_widget(Clear, popup);
                let block = panel_block("Compose text note");
                let inner = block.inner(popup);
                frame.render_widget(block, popup);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(2), Constraint::Min(8), Constraint::Length(2)])
                    .split(inner);

                frame.render_widget(
                    Paragraph::new("Write directly into the Dropply stream. Ctrl+S or Ctrl+Enter sends. Esc cancels.")
                        .style(Style::default().fg(COLOR_MUTED))
                        .wrap(Wrap { trim: false }),
                    chunks[0],
                );
                frame.render_widget(
                    Paragraph::new(compose.draft.as_str())
                        .style(Style::default().fg(COLOR_TEXT))
                        .wrap(Wrap { trim: false })
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(COLOR_BRAND))
                                .title(" Draft "),
                        ),
                    chunks[1],
                );
                let metrics = format!(
                    "{} chars  |  {} lines",
                    compose.draft.chars().count(),
                    compose.draft.lines().count().max(1)
                );
                frame.render_widget(
                    Paragraph::new(metrics)
                        .style(Style::default().fg(COLOR_BRAND)),
                    chunks[2],
                );
            }
            ModalState::ActionPalette(palette) => {
                let popup = centered_rect(62, 54, frame.area());
                frame.render_widget(Clear, popup);
                let block = panel_block("Action palette");
                let inner = block.inner(popup);
                frame.render_widget(block, popup);
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(6)])
                    .split(inner);
                frame.render_widget(
                    Paragraph::new("Choose an action and press Enter. Esc closes.")
                        .style(Style::default().fg(COLOR_MUTED)),
                    rows[0],
                );
                let mut state = ListState::default();
                state.select(Some(palette.selection.min(TuiAction::all().len().saturating_sub(1))));
                let items = TuiAction::all()
                    .into_iter()
                    .map(|action| {
                        ListItem::new(vec![
                            Line::from(Span::styled(
                                action.label(),
                                Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(
                                action.description(),
                                Style::default().fg(COLOR_MUTED),
                            )),
                        ])
                    })
                    .collect::<Vec<_>>();
                let list = List::new(items)
                    .highlight_style(
                        Style::default()
                            .bg(COLOR_BRAND)
                            .fg(COLOR_PANEL)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol(">> ");
                frame.render_stateful_widget(list, rows[1], &mut state);
            }
        }
    }

    fn clamp_selection(&mut self) {
        let filtered_len = self.filtered_item_indices().len();
        if filtered_len == 0 {
            self.stream_selection = 0;
        } else {
            self.stream_selection = self.stream_selection.min(filtered_len.saturating_sub(1));
        }

        let device_len = self
            .data
            .as_ref()
            .and_then(|value| value.snapshot.remote.as_ref().map(|remote| remote.devices.len()))
            .unwrap_or(0);
        if device_len == 0 {
            self.device_selection = 0;
        } else {
            self.device_selection = self.device_selection.min(device_len.saturating_sub(1));
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.tab {
            TuiTab::Stream => {
                let len = self.filtered_item_indices().len();
                if len == 0 {
                    self.stream_selection = 0;
                    return;
                }
                let current = self.stream_selection as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize);
                self.stream_selection = next as usize;
            }
            TuiTab::Devices => {
                let len = self
                    .data
                    .as_ref()
                    .and_then(|value| value.snapshot.remote.as_ref().map(|remote| remote.devices.len()))
                    .unwrap_or(0);
                if len == 0 {
                    self.device_selection = 0;
                    return;
                }
                let current = self.device_selection as isize;
                let next = (current + delta).clamp(0, len.saturating_sub(1) as isize);
                self.device_selection = next as usize;
            }
            _ => {}
        }
    }

    fn move_selection_to_start(&mut self) {
        match self.tab {
            TuiTab::Stream => self.stream_selection = 0,
            TuiTab::Devices => self.device_selection = 0,
            _ => {}
        }
    }

    fn move_selection_to_end(&mut self) {
        match self.tab {
            TuiTab::Stream => {
                let len = self.filtered_item_indices().len();
                self.stream_selection = len.saturating_sub(1);
            }
            TuiTab::Devices => {
                let len = self
                    .data
                    .as_ref()
                    .and_then(|value| value.snapshot.remote.as_ref().map(|remote| remote.devices.len()))
                    .unwrap_or(0);
                self.device_selection = len.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn filtered_item_indices(&self) -> Vec<usize> {
        let Some(data) = self.data.as_ref() else {
            return Vec::new();
        };

        let query = self.stream_filter.trim().to_ascii_lowercase();
        data.snapshot
            .local_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if query.is_empty() {
                    return true;
                }
                let haystack = [
                    item_display_name(item),
                    item.id.clone(),
                    plain_item_type(&item.item_type).to_string(),
                    item.text_preview.clone().unwrap_or_default(),
                    item.mime_type.clone().unwrap_or_default(),
                    smart_drop_filter_text(item),
                ]
                .join(" ")
                .to_ascii_lowercase();
                haystack.contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn selected_item(&self) -> Option<&ItemPayload> {
        let data = self.data.as_ref()?;
        let filtered = self.filtered_item_indices();
        let index = *filtered.get(self.stream_selection)?;
        data.snapshot.local_items.get(index)
    }

    async fn refresh_bundle_preview(&mut self, runtime: &CliRuntime) {
        let Some(item) = self.selected_item() else {
            self.bundle_preview = None;
            return;
        };
        if !is_bundle_item(item) {
            self.bundle_preview = None;
            return;
        }

        if self
            .bundle_preview
            .as_ref()
            .map(|(item_id, _)| item_id == &item.id)
            .unwrap_or(false)
        {
            return;
        }

        match runtime.storage.inspect_conversation_bundle(&item.id).await {
            Ok(details) => {
                self.bundle_preview = Some((item.id.clone(), details));
            }
            Err(error) => {
                self.bundle_preview = None;
                self.set_footer(
                    format!(
                        "Bundle preview failed: {}",
                        shorten_line(&error.to_string(), 100)
                    ),
                    FooterTone::Warn,
                );
            }
        }
    }

    async fn open_item_overlay(&mut self, runtime: &CliRuntime) {
        let Some(item) = self.selected_item().cloned() else {
            return;
        };

        let (title, body) = if is_bundle_item(&item) {
            if self
                .bundle_preview
                .as_ref()
                .map(|(bundle_id, _)| bundle_id != &item.id)
                .unwrap_or(true)
            {
                self.refresh_bundle_preview(runtime).await;
            }
            let body = if let Some((_, details)) = self.bundle_preview.as_ref() {
                build_bundle_overlay_body(&item, details)
            } else {
                "Bundle preview is not available right now.".to_string()
            };
            (format!("Bundle: {}", item_display_name(&item)), body)
        } else if matches!(item.item_type, ItemType::Text) {
            let text_body = item
                .text_content
                .clone()
                .or_else(|| item.text_preview.clone())
                .unwrap_or_else(|| "Text content is not available for this item.".to_string());
            let body = [
                smart_drop_overlay_block(&item),
                String::new(),
                "Text".to_string(),
                text_body,
            ]
            .join("\n");
            (format!("Text: {}", item_display_name(&item)), body)
        } else {
            (
                format!("Item: {}", item_display_name(&item)),
                build_file_overlay_body(&item),
            )
        };

        self.modal = Some(ModalState::Preview(OverlayState {
            title,
            body,
            scroll: 0,
        }));
    }

    async fn submit_composed_text(&mut self, runtime: &CliRuntime) -> AppResult<()> {
        let draft = match self.modal.as_ref() {
            Some(ModalState::ComposeText(compose)) => compose.draft.clone(),
            _ => return Ok(()),
        };

        if draft.trim().is_empty() {
            self.set_footer("Write something first. Empty notes are ignored.", FooterTone::Warn);
            return Ok(());
        }

        let item = runtime.storage.import_text(draft.clone(), None).await?;
        let relay = sync_items_to_relay(runtime, std::slice::from_ref(&item)).await?;
        self.modal = None;
        self.refresh(runtime).await?;
        self.set_footer(
            if relay.remote_paired {
                format!(
                    "Shared note into the stream: {} chars, {} line(s).",
                    draft.chars().count(),
                    draft.lines().count().max(1)
                )
            } else {
                format!(
                    "Saved note locally: {} chars, {} line(s). Pair another device to sync it.",
                    draft.chars().count(),
                    draft.lines().count().max(1)
                )
            },
            FooterTone::Success,
        );
        Ok(())
    }

    async fn update_selected_intent_state(
        &mut self,
        runtime: &CliRuntime,
        intent_state: IntentState,
    ) -> AppResult<()> {
        let Some(item) = self.selected_item().cloned() else {
            return Ok(());
        };

        let Some(updated) = runtime
            .storage
            .update_item_intent_state(&item.id, intent_state)
            .await?
        else {
            self.set_footer("Selected Smart Drop no longer exists.", FooterTone::Warn);
            return Ok(());
        };

        let relay = sync_items_to_relay(runtime, std::slice::from_ref(&updated)).await?;
        self.refresh(runtime).await?;
        self.set_footer(
            if relay.remote_paired {
                format!(
                    "Marked {} as {} and synced the Smart Drop state.",
                    item_display_name(&updated),
                    intent_state_label(intent_state)
                )
            } else {
                format!(
                    "Marked {} as {} locally. Pair another device to sync it.",
                    item_display_name(&updated),
                    intent_state_label(intent_state)
                )
            },
            FooterTone::Success,
        );
        Ok(())
    }

    async fn run_action(&mut self, runtime: &CliRuntime, action: TuiAction) -> AppResult<()> {
        match action {
            TuiAction::Refresh => self.refresh(runtime).await?,
            TuiAction::Pull => {
                let summary = tui_pull(runtime).await?;
                self.refresh(runtime).await?;
                self.set_footer(
                    format!(
                        "Pulled {} new item(s) and removed {} deleted item(s).",
                        summary.imported_count, summary.deleted_count
                    ),
                    FooterTone::Success,
                );
            }
            TuiAction::ComposeText => {
                self.modal = Some(ModalState::ComposeText(ComposeTextState {
                    draft: String::new(),
                }));
                self.set_footer("Compose a note. Ctrl+S sends it. Esc cancels.", FooterTone::Info);
            }
            TuiAction::CopyText => {
                if let Some(item) = self.selected_item() {
                    if matches!(item.item_type, ItemType::Text) {
                        let text = runtime.storage.item_text(&item.id).await?;
                        let Some(text) = text else {
                            self.set_footer("That item no longer has local text content.", FooterTone::Warn);
                            return Ok(());
                        };
                        let mut clipboard = Clipboard::new().map_err(|err| anyhow!(err.to_string()))?;
                        clipboard
                            .set_text(text)
                            .map_err(|err| anyhow!(err.to_string()))?;
                        self.set_footer("Copied the selected text item to your clipboard.", FooterTone::Success);
                    } else {
                        self.set_footer("Copy is available for text items only right now.", FooterTone::Warn);
                    }
                }
            }
            TuiAction::OpenItem => {
                let Some(item_id) = self.selected_item().map(|item| item.id.clone()) else {
                    return Ok(());
                };
                runtime.storage.open_item(&item_id).await?;
                self.set_footer("Opened the selected item with the desktop default app.", FooterTone::Success);
            }
            TuiAction::ExportItem => {
                let Some(item_id) = self.selected_item().map(|item| item.id.clone()) else {
                    return Ok(());
                };
                let exported_path = runtime.storage.export_item_to_downloads(&item_id).await?;
                self.set_footer(
                    format!("Exported selected item to {}.", exported_path),
                    FooterTone::Success,
                );
            }
            TuiAction::MarkPending => {
                self.update_selected_intent_state(runtime, IntentState::Pending).await?;
            }
            TuiAction::MarkCompleted => {
                self.update_selected_intent_state(runtime, IntentState::Completed).await?;
            }
            TuiAction::RevokeIntent => {
                self.update_selected_intent_state(runtime, IntentState::Revoked).await?;
            }
            TuiAction::DeleteItem => {
                if let Some(item) = self.selected_item().cloned() {
                    let item_name = item_display_name(&item);
                    self.modal = Some(ModalState::ConfirmDelete(ConfirmDeleteState {
                        item_id: item.id,
                        item_name,
                    }));
                    self.set_footer("Confirm delete with Enter or y. Esc cancels.", FooterTone::Warn);
                }
            }
            TuiAction::OpenPairPage => {
                let pairing = runtime.pairing()?;
                open_pair_portal(&pairing.pairing_token)?;
                self.set_footer("Opened the Dropply pair page in your browser.", FooterTone::Success);
            }
        }

        Ok(())
    }

    fn stream_preview_text(&self) -> Text<'static> {
        let Some(item) = self.selected_item() else {
            return Text::from("No stream item matches the current filter.");
        };

        let metadata = [
            format!("Type: {}", item_badge(item)),
            format!("Intent: {}", intent_state_label(item.intent_state)),
            format!("Source: {}", source_context_label(item)),
            format!("Smart label: {}", smart_drop_label(item)),
            format!("Tags: {}", smart_drop_tags(item)),
            format!("Suggested: {}", suggested_actions_label(item)),
            format!("Trust: {}", trust_context_label(item)),
            format!("Updated: {}", item.updated_at.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")),
            format!("Device: {}", item.device_id),
            item
                .mime_type
                .as_ref()
                .map(|value| format!("MIME: {value}"))
                .unwrap_or_else(|| "MIME: --".to_string()),
            item
                .size_bytes
                .map(|value| format!("Size: {}", format_bytes(value.max(0) as u64)))
                .unwrap_or_else(|| "Size: --".to_string()),
            item
                .sha256
                .as_ref()
                .map(|value| format!("SHA256: {}", shorten_line(value, 38)))
                .unwrap_or_else(|| "SHA256: --".to_string()),
        ]
        .join("\n");

        let body = if is_bundle_item(item) {
            if let Some((_, details)) = self.bundle_preview.as_ref() {
                let reference_count = details
                    .manifest
                    .entries
                    .iter()
                    .filter(|entry| matches!(entry.role, dropply_lib::models::ConversationBundleEntryRole::Reference))
                    .count();
                let attachment_count = details.manifest.entries.len().saturating_sub(reference_count);
                [
                    metadata,
                    String::new(),
                    format!("Bundle title: {}", details.manifest.title),
                    format!("Source: {}", details.manifest.source_label.clone().unwrap_or_else(|| "Unknown".to_string())),
                    format!("Entries: {} references, {} attachments", reference_count, attachment_count),
                    String::new(),
                    "Transcript preview".to_string(),
                    truncate_block(&details.transcript_markdown, 26),
                ]
                .join("\n")
            } else {
                [metadata, String::new(), "Loading bundle preview...".to_string()].join("\n")
            }
        } else if matches!(item.item_type, ItemType::Text) {
            [
                metadata,
                String::new(),
                "Text preview".to_string(),
                item.text_content
                    .clone()
                    .or_else(|| item.text_preview.clone())
                    .unwrap_or_else(|| "No local text preview is available.".to_string()),
            ]
            .join("\n")
        } else {
            [
                metadata,
                String::new(),
                "File preview".to_string(),
                item.storage_path
                    .clone()
                    .map(|value| format!("Stored at: {value}"))
                    .unwrap_or_else(|| "Stored path unavailable.".to_string()),
                String::new(),
                "Use Enter to inspect metadata in a larger overlay or press e to export this item.".to_string(),
            ]
            .join("\n")
        };

        Text::from(body)
    }

    fn set_footer(&mut self, text: impl Into<String>, tone: FooterTone) {
        self.footer = FooterMessage {
            text: text.into(),
            tone,
        };
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> AppResult<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

struct PullSummary {
    imported_count: usize,
    deleted_count: usize,
}

async fn tui_pull(runtime: &CliRuntime) -> AppResult<PullSummary> {
    let pairing = runtime.pairing()?;
    let _ = register_pairing_device(runtime, &pairing).await?;
    let remote = fetch_relay_pull(runtime, &pairing).await?;

    let local_items = runtime.storage.list_items().await?;
    let mut existing_ids = local_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();

    let mut deleted_count = 0usize;
    let mut imported_count = 0usize;

    for item in remote.items {
        if item.deleted.unwrap_or(false) {
            if existing_ids.remove(&item.id) {
                runtime.storage.delete_item(&item.id).await?;
                deleted_count += 1;
            }
            continue;
        }

        if existing_ids.contains(&item.id) {
            continue;
        }

        if matches!(item.item_type, ItemType::Text) || item.bytes_b64.is_some() {
            let _ = runtime.storage.import_relay_item(item).await?;
        } else {
            let _ = pull_relay_blob_to_storage(runtime, &pairing, item).await?;
        }
        imported_count += 1;
    }

    Ok(PullSummary {
        imported_count,
        deleted_count,
    })
}

fn panel_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BRAND))
        .style(Style::default().bg(COLOR_PANEL))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD),
        ))
}

fn loading_panel(message: &str) -> Paragraph<'static> {
    Paragraph::new(message.to_string())
        .alignment(Alignment::Center)
        .style(Style::default().fg(COLOR_TEXT))
        .block(panel_block("Loading"))
}

fn render_metric_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: impl Into<String>,
    subtitle: &str,
    color: Color,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .style(Style::default().bg(COLOR_PANEL_MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(value.into())
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Left),
        content[0],
    );
    frame.render_widget(
        Paragraph::new(subtitle)
            .style(Style::default().fg(COLOR_MUTED))
            .wrap(Wrap { trim: false }),
        content[1],
    );
}

fn centered_rect(width_pct: u16, height_pct: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}

fn item_badge(item: &ItemPayload) -> String {
    if is_bundle_item(item) {
        return "BUNDLE".to_string();
    }
    match item.item_type {
        ItemType::Text => "TEXT".to_string(),
        ItemType::Image => "IMAGE".to_string(),
        ItemType::File => "FILE".to_string(),
    }
}

fn smart_drop_label(item: &ItemPayload) -> String {
    item.semantic_context
        .as_ref()
        .map(|semantic| semantic.primary_label.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Smart Drop".to_string())
}

fn smart_drop_tags(item: &ItemPayload) -> String {
    item.semantic_context
        .as_ref()
        .map(|semantic| {
            if semantic.tags.is_empty() {
                "--".to_string()
            } else {
                semantic.tags.join(", ")
            }
        })
        .unwrap_or_else(|| "--".to_string())
}

fn suggested_actions_label(item: &ItemPayload) -> String {
    let actions = item
        .suggested_actions
        .iter()
        .filter(|action| action.enabled)
        .take(4)
        .map(|action| suggested_action_label(action.id).to_string())
        .collect::<Vec<_>>();

    if actions.is_empty() {
        "--".to_string()
    } else {
        actions.join(", ")
    }
}

fn suggested_action_label(action_id: SuggestedActionId) -> &'static str {
    match action_id {
        SuggestedActionId::Copy => "copy",
        SuggestedActionId::Open => "open",
        SuggestedActionId::Download => "download",
        SuggestedActionId::OpenBundle => "open bundle",
        SuggestedActionId::SendToDevice => "send to device",
        SuggestedActionId::ResumeLater => "resume later",
        SuggestedActionId::SummarizeLater => "summarize later",
    }
}

fn source_context_label(item: &ItemPayload) -> String {
    let Some(source_context) = item.source_context.as_ref() else {
        return "unknown".to_string();
    };

    let kind = source_kind_label(source_context.source_kind);
    source_context
        .source_app
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|app| format!("{kind} from {app}"))
        .unwrap_or_else(|| kind.to_string())
}

fn source_kind_label(source_kind: SourceKind) -> &'static str {
    match source_kind {
        SourceKind::Composer => "composer",
        SourceKind::Paste => "paste",
        SourceKind::DragDrop => "drag drop",
        SourceKind::FilePicker => "file picker",
        SourceKind::BrowserShare => "browser share",
        SourceKind::Relay => "relay",
        SourceKind::Direct => "direct",
    }
}

fn trust_context_label(item: &ItemPayload) -> String {
    let Some(trust_context) = item.trust_context.as_ref() else {
        return "local-first".to_string();
    };

    let provenance = match trust_context.provenance {
        TrustProvenance::Local => "local",
        TrustProvenance::PairedDevice => "paired device",
        TrustProvenance::BrowserExtension => "browser extension",
    };

    if trust_context.revoked_at.is_some() {
        format!("{provenance}, revoked")
    } else {
        provenance.to_string()
    }
}

fn intent_state_label(intent_state: IntentState) -> &'static str {
    match intent_state {
        IntentState::Captured => "captured",
        IntentState::Pending => "pending",
        IntentState::Sent => "sent",
        IntentState::Resumed => "resumed",
        IntentState::Completed => "done",
        IntentState::Revoked => "revoked",
    }
}

fn smart_drop_filter_text(item: &ItemPayload) -> String {
    [
        smart_drop_label(item),
        smart_drop_tags(item),
        source_context_label(item),
        trust_context_label(item),
        intent_state_label(item.intent_state).to_string(),
    ]
    .join(" ")
}

fn smart_drop_overlay_block(item: &ItemPayload) -> String {
    let summary = item
        .semantic_context
        .as_ref()
        .and_then(|semantic| semantic.summary.clone())
        .unwrap_or_else(|| "--".to_string());
    let source_url = item
        .source_context
        .as_ref()
        .and_then(|source| source.source_url.clone())
        .unwrap_or_else(|| "--".to_string());

    [
        "Smart Drop".to_string(),
        format!("Intent: {}", intent_state_label(item.intent_state)),
        format!("Label: {}", smart_drop_label(item)),
        format!("Source: {}", source_context_label(item)),
        format!("Source URL: {source_url}"),
        format!("Tags: {}", smart_drop_tags(item)),
        format!("Suggested: {}", suggested_actions_label(item)),
        format!("Trust: {}", trust_context_label(item)),
        format!("Summary: {summary}"),
    ]
    .join("\n")
}

fn is_bundle_item(item: &ItemPayload) -> bool {
    item.mime_type.as_deref() == Some(CONVERSATION_BUNDLE_MIME_TYPE)
        || item
            .name
            .as_deref()
            .map(is_conversation_bundle_name)
            .unwrap_or(false)
}

fn summarize_log_title(entry: &LogEntry) -> String {
    let subject = entry
        .payload
        .get("name")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            entry
                .payload
                .get("item_id")
                .and_then(serde_json::Value::as_str)
        })
        .map(|value| shorten_line(value, 28))
        .unwrap_or_else(|| short_id(&entry.item_id));

    format!("{} {}", entry.op.to_ascii_uppercase(), subject)
}

fn summarize_log_detail(entry: &LogEntry) -> String {
    let device = short_id(&entry.device_id);
    let item_kind = entry
        .payload
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("item");
    format!("{item_kind}  |  device {device}  |  item {}", short_id(&entry.item_id))
}

fn format_relative_time(timestamp: DateTime<Utc>) -> String {
    let delta = Local::now() - timestamp.with_timezone(&Local);
    if delta.num_seconds() < 60 {
        return format!("{}s ago", delta.num_seconds().max(0));
    }
    if delta.num_minutes() < 60 {
        return format!("{}m ago", delta.num_minutes().max(0));
    }
    if delta.num_hours() < 24 {
        return format!("{}h ago", delta.num_hours().max(0));
    }
    format!("{}d ago", delta.num_days().max(0))
}

fn format_last_seen(timestamp_ms: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
        .map(format_relative_time)
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_compact_pair_qr(data: &str) -> AppResult<String> {
    let qr = QrCode::encode_text(data, QrCodeEcc::Medium)
        .map_err(|_| anyhow!("failed to encode compact terminal QR code"))?;
    let border = 1;
    let size = qr.size();
    let mut out = String::new();

    let mut y = -border;
    while y < size + border {
        let mut x = -border;
        while x < size + border {
            let top_left = qr_module(&qr, size, x, y);
            let top_right = qr_module(&qr, size, x + 1, y);
            let bottom_left = qr_module(&qr, size, x, y + 1);
            let bottom_right = qr_module(&qr, size, x + 1, y + 1);

            let glyph = quadrant_glyph(top_left, top_right, bottom_left, bottom_right);
            out.push(glyph);
            x += 2;
        }
        out.push('\n');
        y += 2;
    }

    Ok(out)
}

fn qr_module(qr: &QrCode, size: i32, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < size && y < size && qr.get_module(x, y)
}

fn quadrant_glyph(top_left: bool, top_right: bool, bottom_left: bool, bottom_right: bool) -> char {
    match (
        top_left as u8,
        top_right as u8,
        bottom_left as u8,
        bottom_right as u8,
    ) {
        (0, 0, 0, 0) => ' ',
        (1, 0, 0, 0) => '▘',
        (0, 1, 0, 0) => '▝',
        (0, 0, 1, 0) => '▖',
        (0, 0, 0, 1) => '▗',
        (1, 1, 0, 0) => '▀',
        (0, 0, 1, 1) => '▄',
        (1, 0, 1, 0) => '▌',
        (0, 1, 0, 1) => '▐',
        (1, 0, 0, 1) => '▚',
        (0, 1, 1, 0) => '▞',
        (1, 1, 1, 0) => '▛',
        (1, 1, 0, 1) => '▜',
        (1, 0, 1, 1) => '▙',
        (0, 1, 1, 1) => '▟',
        (1, 1, 1, 1) => '█',
        _ => ' ',
    }
}

fn truncate_block(value: &str, max_lines: usize) -> String {
    let lines = value.lines().take(max_lines).collect::<Vec<_>>();
    let truncated = value.lines().count() > max_lines;
    if truncated {
        format!("{}\n\n...truncated for preview", lines.join("\n"))
    } else {
        lines.join("\n")
    }
}

fn build_bundle_overlay_body(item: &ItemPayload, details: &ConversationBundleDetailsPayload) -> String {
    let mut body = vec![
        format!("File name: {}", item_display_name(item)),
        smart_drop_overlay_block(item),
        String::new(),
        format!("Bundle title: {}", details.manifest.title),
        format!(
            "Source: {}",
            details
                .manifest
                .source_label
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
        ),
        details
            .manifest
            .source_url
            .clone()
            .map(|value| format!("Source URL: {value}"))
            .unwrap_or_else(|| "Source URL: --".to_string()),
        format!(
            "Created: {}",
            details
                .manifest
                .created_at
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
        ),
        format!("Transcript SHA256: {}", details.manifest.transcript_sha256),
        String::new(),
        "Transcript".to_string(),
        details.transcript_markdown.clone(),
        String::new(),
        "Entries".to_string(),
    ];

    for entry in &details.manifest.entries {
        body.push(format!(
            "- {}  [{} | {} | {}]",
            entry.path,
            match entry.role {
                dropply_lib::models::ConversationBundleEntryRole::Reference => "reference",
                dropply_lib::models::ConversationBundleEntryRole::Attachment => "attachment",
            },
            entry
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            format_bytes(entry.size_bytes.max(0) as u64)
        ));
    }

    body.join("\n")
}

fn build_file_overlay_body(item: &ItemPayload) -> String {
    [
        format!("Name: {}", item_display_name(item)),
        smart_drop_overlay_block(item),
        String::new(),
        format!("Type: {}", item_badge(item)),
        format!(
            "Updated: {}",
            item.updated_at
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
        ),
        format!("Device ID: {}", item.device_id),
        item
            .mime_type
            .clone()
            .map(|value| format!("MIME: {value}"))
            .unwrap_or_else(|| "MIME: --".to_string()),
        item
            .size_bytes
            .map(|value| format!("Size: {}", format_bytes(value.max(0) as u64)))
            .unwrap_or_else(|| "Size: --".to_string()),
        item
            .sha256
            .clone()
            .map(|value| format!("SHA256: {value}"))
            .unwrap_or_else(|| "SHA256: --".to_string()),
        item
            .storage_path
            .clone()
            .map(|value| format!("Storage path: {value}"))
            .unwrap_or_else(|| "Storage path: --".to_string()),
    ]
    .join("\n")
}
