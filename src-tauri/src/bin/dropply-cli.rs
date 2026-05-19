use std::collections::HashSet;
use std::env;
use std::error::Error as _;
use std::fmt::{Display, Formatter};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Local};
use dropply_lib::models::{
    ConversationBundleSourcePayload, ImportConversationBundlePayload, ImportPathPayload,
    ItemPayload, ItemType, PairingInfo, RelayBlobPayload, RelayItemPayload,
};
use dropply_lib::storage::sandbox::ShareBundleOrigin;
use dropply_lib::{AppResult, Storage};
use futures_util::{stream, StreamExt, TryStreamExt};
use qrcodegen::{QrCode, QrCodeEcc};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[path = "../cli_tui.rs"]
mod dropply_cli_tui;

const DEFAULT_API_BASE_URL: &str = "https://dropply-backend.fortifie.com";
const DEFAULT_PAIR_PORTAL_URL: &str = "https://dropply.ca/pair";
const CLI_CONFIG_FILE: &str = "dropply-cli.toml";
const CLI_STORAGE_APP_NAME: &str = "dropply\\cli";
const CLI_STORAGE_FALLBACK_APP_NAME: &str = "dropply-cli";
const CLI_DEVICE_TYPE: &str = "desktop";
const CLI_DEVICE_LABEL: &str = "Dropply CLI";
const MAX_RELAY_SNAPSHOT_JSON_CHARS: usize = 480 * 1024;
const MAX_RELAY_SNAPSHOT_INLINE_B64_CHARS: usize = 192 * 1024;
const BASE_RELAY_BLOB_CHUNK_BYTES: usize = 128 * 1024;
const MEDIUM_RELAY_BLOB_CHUNK_BYTES: usize = 256 * 1024;
const LARGE_RELAY_BLOB_CHUNK_BYTES: usize = 512 * 1024;
const SMALL_RELAY_BLOB_PARALLEL_WORKERS: usize = 2;
const MEDIUM_RELAY_BLOB_PARALLEL_WORKERS: usize = 3;
const LARGE_RELAY_BLOB_PARALLEL_WORKERS: usize = 4;
const MAX_RELAY_REQUEST_RETRIES: usize = 5;
const RELAY_RETRY_BASE_DELAY_MS: u64 = 900;
const RELAY_RETRY_MAX_DELAY_MS: u64 = 8_000;
const STATUS_WATCH_INTERVAL: Duration = Duration::from_secs(2);
const HEADER_WIDTH: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Pretty,
    Json,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TransportMode {
    Auto,
    P2p,
    Relay,
}

impl Default for TransportMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl Display for TransportMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::P2p => write!(f, "p2p"),
            Self::Relay => write!(f, "relay"),
        }
    }
}

impl TransportMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "p2p" => Some(Self::P2p),
            "relay" => Some(Self::Relay),
            _ => None,
        }
    }

    fn pair_preference(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::P2p => Some("p2p"),
            Self::Relay => Some("relay"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionShell {
    PowerShell,
    Bash,
    Zsh,
}

impl CompletionShell {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "powershell" | "pwsh" => Some(Self::PowerShell),
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CliConfig {
    transport_mode: TransportMode,
    api_base_url: Option<String>,
}

#[derive(Debug)]
enum Command {
    Pair {
        token: Option<String>,
        reset: bool,
        open: bool,
    },
    Send {
        paths: Vec<String>,
    },
    SendText {
        text: Option<String>,
    },
    SendBundle {
        title: Option<String>,
        source_label: Option<String>,
        source_url: Option<String>,
        transcript_file: Option<String>,
        files: Vec<String>,
        attachments: Vec<String>,
    },
    List,
    Pull,
    Tui,
    Status {
        watch: bool,
    },
    Transport {
        mode: Option<TransportMode>,
    },
    Completions {
        shell: CompletionShell,
    },
    Help,
}

struct ParsedInvocation {
    output_mode: OutputMode,
    command: Command,
    show_intro: bool,
}

#[derive(Debug, Deserialize)]
struct PairDevice {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "deviceType")]
    device_type: String,
    label: String,
    #[serde(rename = "lastSeenAt")]
    last_seen_at: i64,
    #[serde(rename = "transportPreference")]
    transport_preference: String,
}

#[derive(Debug, Deserialize)]
struct PairStatus {
    devices: Vec<PairDevice>,
    paired: bool,
    #[serde(rename = "pairedDeviceCount")]
    paired_device_count: usize,
    #[serde(rename = "itemCount")]
    item_count: usize,
}

#[derive(Debug, Serialize)]
struct RelaySyncSummary {
    remote_paired: bool,
    snapshot_pushed: bool,
}

#[derive(Debug, Deserialize)]
struct RelayPullResponse {
    items: Vec<RelayItemPayload>,
}

#[derive(Debug, Deserialize)]
struct RelayBlobStatusRange {
    start: usize,
    end: usize,
}

#[derive(Debug, Deserialize)]
struct RelayBlobUploadStatus {
    resumable: bool,
    #[serde(rename = "receivedRanges")]
    received_ranges: Vec<RelayBlobStatusRange>,
}

#[derive(Debug, Deserialize)]
struct RelayBlobChunkResponse {
    #[serde(rename = "totalChunks")]
    total_chunks: usize,
    bytes_b64: String,
}

struct CliRuntime {
    storage: Storage,
    client: Client,
    config: CliConfig,
    api_base_url: String,
    output_mode: OutputMode,
    used_storage_fallback: bool,
}

struct StatusSnapshot {
    pairing: PairingInfo,
    local_items: Vec<ItemPayload>,
    remote: Option<PairStatus>,
    transport_mode: TransportMode,
    api_base_url: String,
    data_dir: String,
    used_storage_fallback: bool,
}

#[derive(Clone)]
struct TransferMeter {
    verb: &'static str,
    label: String,
    total_bytes: u64,
    total_chunks: usize,
    started_at: Instant,
}

#[derive(Default)]
struct ChunkProgressState {
    completed_chunks: usize,
    completed_bytes: u64,
}

struct RetryEvent {
    attempt: usize,
    total_attempts: usize,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run_cli().await {
        eprintln!("Dropply CLI error: {error}");
        let mut source = error.source();
        while let Some(cause) = source {
            eprintln!("  caused by: {cause}");
            source = cause.source();
        }
        maybe_wait_for_explorer_close();
        std::process::exit(1);
    }
}

async fn run_cli() -> AppResult<()> {
    let invocation = parse_invocation(env::args().skip(1).collect::<Vec<_>>())?;
    if invocation.show_intro && invocation.output_mode == OutputMode::Pretty {
        play_intro_animation();
    }

    match invocation.command {
        Command::Help => {
            print_help(invocation.output_mode)?;
            if invocation.output_mode == OutputMode::Pretty {
                maybe_wait_for_explorer_close();
            }
            return Ok(());
        }
        Command::Completions { shell } => {
            print_completions(shell)?;
            return Ok(());
        }
        _ => {}
    }

    let runtime = CliRuntime::load(invocation.output_mode).await?;

    match invocation.command {
        Command::Pair { token, reset, open } => run_pair_command(&runtime, token, reset, open).await,
        Command::Send { paths } => run_send_command(&runtime, paths).await,
        Command::SendText { text } => run_send_text_command(&runtime, text).await,
        Command::SendBundle {
            title,
            source_label,
            source_url,
            transcript_file,
            files,
            attachments,
        } => {
            run_send_bundle_command(
                &runtime,
                title,
                source_label,
                source_url,
                transcript_file,
                files,
                attachments,
            )
            .await
        }
        Command::List => run_list_command(&runtime).await,
        Command::Pull => run_pull_command(&runtime).await,
        Command::Tui => dropply_cli_tui::run_tui_command(&runtime).await,
        Command::Status { watch } => run_status_command(&runtime, watch).await,
        Command::Transport { mode } => run_transport_command(&runtime, mode).await,
        Command::Completions { .. } | Command::Help => Ok(()),
    }
}

impl CliRuntime {
    async fn load(output_mode: OutputMode) -> AppResult<Self> {
        let (storage, used_storage_fallback) = open_cli_storage().await?;
        let config = load_cli_config(storage.base_dir().join(CLI_CONFIG_FILE))?;
        let api_base_url = env::var("DROPPLY_API_BASE_URL")
            .ok()
            .or_else(|| config.api_base_url.clone())
            .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string());

        let client = Client::builder()
            .user_agent(format!("dropply-cli/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to initialize HTTP client")?;

        Ok(Self {
            storage,
            client,
            config,
            api_base_url,
            output_mode,
            used_storage_fallback,
        })
    }

    fn config_path(&self) -> PathBuf {
        self.storage.base_dir().join(CLI_CONFIG_FILE)
    }

    fn pairing(&self) -> AppResult<PairingInfo> {
        self.storage.pairing()
    }

    fn is_json(&self) -> bool {
        self.output_mode == OutputMode::Json
    }

    fn can_render_pretty(&self) -> bool {
        !self.is_json() && io::stdout().is_terminal()
    }

    fn emit_json<T: Serialize>(&self, value: &T) -> AppResult<()> {
        println!("{}", serde_json::to_string(value)?);
        Ok(())
    }
}

async fn open_cli_storage() -> AppResult<(Storage, bool)> {
    match Storage::new(CLI_STORAGE_APP_NAME).await {
        Ok(storage) => Ok((storage, false)),
        Err(primary_error) => match Storage::new(CLI_STORAGE_FALLBACK_APP_NAME).await {
            Ok(storage) => Ok((storage, true)),
            Err(fallback_error) => Err(anyhow!(
                "failed to open Dropply storage: primary path error: {}; fallback path error: {}",
                primary_error,
                fallback_error
            )
            .into()),
        },
    }
}

impl TransferMeter {
    fn new(verb: &'static str, label: impl Into<String>, total_bytes: u64, total_chunks: usize) -> Self {
        Self {
            verb,
            label: label.into(),
            total_bytes,
            total_chunks: total_chunks.max(1),
            started_at: Instant::now(),
        }
    }

    fn render(&self, phase: &str, completed_bytes: u64, completed_chunks: usize, retry: Option<RetryEvent>) {
        if !io::stdout().is_terminal() {
            return;
        }

        let total_bytes = self.total_bytes.max(1);
        let clamped_bytes = completed_bytes.min(total_bytes);
        let clamped_chunks = completed_chunks.min(self.total_chunks);
        let percent = ((clamped_bytes as f64 / total_bytes as f64) * 100.0).round() as usize;
        let bar = progress_bar(percent, 22);
        let elapsed = self.started_at.elapsed().as_secs_f64().max(0.05);
        let bytes_per_second = clamped_bytes as f64 / elapsed;
        let remaining_bytes = total_bytes.saturating_sub(clamped_bytes);
        let eta = if bytes_per_second > 0.0 {
            Some(Duration::from_secs_f64(remaining_bytes as f64 / bytes_per_second))
        } else {
            None
        };
        let phase_label = phase_badge(phase);
        let retry_text = retry
            .map(|details| format!(" retry {}/{}", details.attempt, details.total_attempts))
            .unwrap_or_default();

        let mut stdout = io::stdout();
        let _ = write!(
            stdout,
            "\r{} {:<4} {:<9} {:<20} {} {:>3}% {} / {} {:>10}/s ETA {:>7} {:>3}/{}{}",
            info_tag(),
            self.verb,
            phase_label,
            shorten_line(&self.label, 20),
            bar,
            percent,
            format_bytes(clamped_bytes),
            format_bytes(total_bytes),
            format_bytes(bytes_per_second.round() as u64),
            eta.map(format_duration_compact)
                .unwrap_or_else(|| "--".to_string()),
            clamped_chunks,
            self.total_chunks,
            retry_text
        );
        let _ = stdout.flush();

        if phase == "complete" {
            let _ = writeln!(stdout);
        }
    }
}

fn load_cli_config(path: PathBuf) -> AppResult<CliConfig> {
    match std::fs::read_to_string(path) {
        Ok(raw) => toml::from_str(&raw)
            .context("failed to parse CLI config")
            .map_err(Into::into),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CliConfig::default()),
        Err(error) => Err(error.into()),
    }
}

fn save_cli_config(path: PathBuf, config: &CliConfig) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string(config).context("failed to serialize CLI config")?;
    std::fs::write(path, serialized)?;
    Ok(())
}

fn parse_invocation(args: Vec<String>) -> AppResult<ParsedInvocation> {
    let mut output_mode = OutputMode::Pretty;
    let mut filtered = Vec::new();
    for arg in args {
        if arg == "--json" {
            output_mode = OutputMode::Json;
        } else {
            filtered.push(arg);
        }
    }

    let show_intro = filtered.is_empty();
    let command = parse_command(filtered)?;
    Ok(ParsedInvocation {
        output_mode,
        command,
        show_intro,
    })
}

fn parse_command(args: Vec<String>) -> AppResult<Command> {
    let Some(command) = args.first().map(|value| value.as_str()) else {
        return Ok(Command::Help);
    };

    match command {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "pair" => {
            let mut token = None;
            let mut reset = false;
            let mut open = false;

            for arg in args.iter().skip(1) {
                match arg.as_str() {
                    "--reset" => reset = true,
                    "--open" => open = true,
                    value if token.is_none() => token = Some(value.to_string()),
                    _ => {
                        return Err(anyhow!(
                            "usage: dropply-cli pair [TOKEN] [--reset] [--open]"
                        )
                        .into())
                    }
                }
            }

            Ok(Command::Pair { token, reset, open })
        }
        "send" => {
            let paths = args.iter().skip(1).cloned().collect::<Vec<_>>();
            if paths.is_empty() {
                return Err(anyhow!("usage: dropply-cli send <file> [more files]").into());
            }
            Ok(Command::Send { paths })
        }
        "send-text" => Ok(Command::SendText {
            text: if args.len() > 1 {
                Some(args.iter().skip(1).cloned().collect::<Vec<_>>().join(" "))
            } else {
                None
            },
        }),
        "send-bundle" => {
            let mut title = None;
            let mut source_label = None;
            let mut source_url = None;
            let mut transcript_file = None;
            let mut files = Vec::new();
            let mut attachments = Vec::new();

            let mut index = 1;
            while index < args.len() {
                let flag = args[index].as_str();
                let value = args.get(index + 1).cloned().ok_or_else(|| {
                    anyhow!(
                        "usage: dropply-cli send-bundle [--title TITLE] [--source LABEL] [--url URL] [--transcript-file PATH] [--file PATH]... [--attachment PATH]..."
                    )
                })?;

                match flag {
                    "--title" => title = Some(value),
                    "--source" => source_label = Some(value),
                    "--url" => source_url = Some(value),
                    "--transcript-file" => transcript_file = Some(value),
                    "--file" => files.push(value),
                    "--attachment" => attachments.push(value),
                    _ => {
                        return Err(anyhow!(
                            "usage: dropply-cli send-bundle [--title TITLE] [--source LABEL] [--url URL] [--transcript-file PATH] [--file PATH]... [--attachment PATH]..."
                        )
                        .into())
                    }
                }

                index += 2;
            }

            Ok(Command::SendBundle {
                title,
                source_label,
                source_url,
                transcript_file,
                files,
                attachments,
            })
        }
        "list" => Ok(Command::List),
        "pull" => Ok(Command::Pull),
        "tui" => Ok(Command::Tui),
        "status" => {
            if args.len() == 1 {
                return Ok(Command::Status { watch: false });
            }

            if args.len() == 2 && args[1] == "--watch" {
                return Ok(Command::Status { watch: true });
            }

            Err(anyhow!("usage: dropply-cli status [--watch]").into())
        }
        "transport" => {
            if args.len() == 1 {
                return Ok(Command::Transport { mode: None });
            }

            if args.len() == 3 && args[1] == "--mode" {
                let mode = TransportMode::parse(&args[2]).ok_or_else(|| {
                    anyhow!(
                        "invalid transport mode '{}'; use auto, p2p, or relay",
                        args[2]
                    )
                })?;
                return Ok(Command::Transport { mode: Some(mode) });
            }

            Err(anyhow!("usage: dropply-cli transport --mode <auto|p2p|relay>").into())
        }
        "completions" => {
            let shell = args
                .get(1)
                .and_then(|value| CompletionShell::parse(value))
                .ok_or_else(|| anyhow!("usage: dropply-cli completions <powershell|bash|zsh>"))?;
            Ok(Command::Completions { shell })
        }
        other => Err(anyhow!("unknown command '{other}'. Run 'dropply-cli help'.").into()),
    }
}

fn print_help(output_mode: OutputMode) -> AppResult<()> {
    if output_mode == OutputMode::Json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "name": "dropply-cli",
                "tagline": "Advanced companion for fast device-to-device handoff",
                "commands": {
                    "connect": [
                        "pair [TOKEN]",
                        "pair --reset",
                        "pair --open"
                    ],
                    "send": [
                        "send <file> [more files]",
                        "send-text [text]",
                        "send-bundle [--title TITLE] [--source LABEL] [--url URL] [--transcript-file PATH] [--file PATH]... [--attachment PATH]..."
                    ],
                    "inspect": [
                        "list",
                        "pull",
                        "tui",
                        "status",
                        "status --watch",
                        "transport --mode <auto|p2p|relay>"
                    ],
                    "setup": [
                        "completions <powershell|bash|zsh>"
                    ]
                },
                "notes": [
                    "send-text reads stdin when no text argument is provided",
                    "send-bundle reads stdin when --transcript-file is omitted",
                    "pair shows a shareable browser URL and terminal QR code",
                    "the CLI keeps its own local Dropply cache and pairs into the same session token when linked",
                    "use dropply-cli --json ... for compact scripting output"
                ]
            }))?
        );
        return Ok(());
    }

    print_banner("Advanced companion for fast device-to-device handoff.");
    print_block_header("Connect");
    print_command_row("dropply-cli pair", "Show your token, QR, and paired session status.");
    print_command_row("dropply-cli pair TOKEN", "Switch to a different pair session token.");
    print_command_row("dropply-cli pair --open", "Open the browser pairing page for your token.");
    print_command_row("dropply-cli pair --reset", "Mint a fresh token for this CLI device.");

    print_block_header("Send");
    print_command_row(
        "dropply-cli send <file> [more files]",
        "Queue files, sync relay blobs, and publish into your Dropply stream.",
    );
    print_command_row(
        "dropply-cli send-text [text]",
        "Send a note directly, or prompt interactively when no text is provided.",
    );
    print_command_row(
        "dropply-cli send-bundle [--title TITLE] [--source LABEL] [--url URL] [--transcript-file PATH] [--file PATH]... [--attachment PATH]...",
        "Package a transcript, referenced files, and docs into one conversation bundle item.",
    );

    print_block_header("Inspect");
    print_command_row("dropply-cli list", "Show your local Dropply cache.");
    print_command_row("dropply-cli pull", "Pull newly shared items from the active pair session.");
    print_command_row("dropply-cli tui", "Open the full terminal dashboard for stream, devices, and activity.");
    print_command_row("dropply-cli status", "Show device, pair, relay, and cache status.");
    print_command_row("dropply-cli status --watch", "Live-refresh the session view until you exit.");
    print_command_row(
        "dropply-cli transport --mode <auto|p2p|relay>",
        "Set the preferred transport label Dropply advertises for this device.",
    );

    print_block_header("Setup");
    print_command_row(
        "dropply-cli completions powershell",
        "Emit shell completions so install scripts can wire them in automatically.",
    );
    print_command_row("dropply-cli --json ...", "Switch any result-shaped command into scriptable JSON.");

    print_block_header("Examples");
    println!("  dropply-cli pair");
    println!("  dropply-cli send \"C:\\Users\\alexj\\Videos\\clip.mp4\"");
    println!("  dropply-cli send-text");
    println!("  Get-Content .\\notes.txt | dropply-cli send-text");
    println!("  dropply-cli send-bundle --title \"ChatGPT session\" --source ChatGPT --transcript-file .\\conversation.md --file src\\components\\Canvas.tsx --attachment .\\summary.md");
    println!("  dropply-cli status --watch");
    println!("  dropply-cli tui");

    print_block_header("Notes");
    println!("  {} CLI v1 still uses the hosted relay session path for transfer orchestration.", info_tag());
    println!("  {} The CLI keeps its own local Dropply cache under AppData and pairs into the same token when linked.", info_tag());
    println!("  {} Double-click launch shows a quick intro before this help screen.", info_tag());
    Ok(())
}

fn print_completions(shell: CompletionShell) -> AppResult<()> {
    match shell {
        CompletionShell::PowerShell => {
            println!(
                "{}",
                r#"
Register-ArgumentCompleter -Native -CommandName dropply-cli -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)

  $elements = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.Extent.Text })
  $root = @('pair', 'send', 'send-text', 'send-bundle', 'list', 'pull', 'tui', 'status', 'transport', 'completions', 'help')
  $candidates = @()

  if ($elements.Count -le 1) {
    $candidates = $root
  } else {
    switch ($elements[0]) {
      'pair' {
        $candidates = @('--reset', '--open')
      }
      'status' {
        $candidates = @('--watch')
      }
      'send-bundle' {
        $candidates = @('--title', '--source', '--url', '--transcript-file', '--file', '--attachment')
      }
      'transport' {
        if ($elements.Count -eq 2) {
          $candidates = @('--mode')
        } elseif ($elements.Count -ge 3 -and $elements[1] -eq '--mode') {
          $candidates = @('auto', 'p2p', 'relay')
        }
      }
      'completions' {
        $candidates = @('powershell', 'bash', 'zsh')
      }
    }
  }

  $candidates |
    Where-Object { $_ -like "$wordToComplete*" } |
    ForEach-Object {
      [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}
"#
                .trim()
            );
        }
        CompletionShell::Bash => {
            println!(
                "{}",
                r#"
_dropply_cli() {
  local cur prev
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"

  case "${COMP_WORDS[1]}" in
    pair)
      COMPREPLY=( $(compgen -W "--reset --open" -- "$cur") )
      return
      ;;
    send-bundle)
      COMPREPLY=( $(compgen -W "--title --source --url --transcript-file --file --attachment" -- "$cur") )
      return
      ;;
    status)
      COMPREPLY=( $(compgen -W "--watch" -- "$cur") )
      return
      ;;
    transport)
      if [[ "$prev" == "--mode" ]]; then
        COMPREPLY=( $(compgen -W "auto p2p relay" -- "$cur") )
      else
        COMPREPLY=( $(compgen -W "--mode" -- "$cur") )
      fi
      return
      ;;
    completions)
      COMPREPLY=( $(compgen -W "powershell bash zsh" -- "$cur") )
      return
      ;;
  esac

  COMPREPLY=( $(compgen -W "pair send send-text send-bundle list pull tui status transport completions help" -- "$cur") )
}
complete -F _dropply_cli dropply-cli
"#
                .trim()
            );
        }
        CompletionShell::Zsh => {
            println!(
                "{}",
                r#"
#compdef dropply-cli

local -a commands
commands=(
  'pair:Show pairing status and QR'
  'send:Queue files into Dropply'
  'send-text:Send terminal text'
  'send-bundle:Send a bundled conversation transcript'
  'list:Show local cache'
  'pull:Pull newly shared items'
  'tui:Open the Dropply terminal dashboard'
  'status:Show session status'
  'transport:Set the preferred transport mode'
  'completions:Print shell completions'
  'help:Show help'
)

case $words[2] in
  pair)
    _describe 'pair flags' '--reset --open'
    ;;
  send-bundle)
    _describe 'send-bundle flags' '--title --source --url --transcript-file --file --attachment'
    ;;
  status)
    _describe 'status flags' '--watch'
    ;;
  transport)
    if [[ $words[CURRENT-1] == '--mode' ]]; then
      _describe 'transport modes' 'auto p2p relay'
    else
      _describe 'transport flags' '--mode'
    fi
    ;;
  completions)
    _describe 'shells' 'powershell bash zsh'
    ;;
  *)
    _describe 'commands' commands
    ;;
esac
"#
                .trim()
            );
        }
    }

    Ok(())
}

async fn run_pair_command(
    runtime: &CliRuntime,
    token: Option<String>,
    reset: bool,
    open: bool,
) -> AppResult<()> {
    if reset {
        let new_token = runtime.storage.reset_pairing_token()?;
        let pairing = runtime.pairing()?;
        let status = register_pairing_device(runtime, &pairing).await.ok();
        if runtime.is_json() {
            return runtime.emit_json(&serde_json::json!({
                "ok": true,
                "reset": true,
                "device_id": pairing.device_id,
                "pair_token": new_token,
                "transport_mode": runtime.config.transport_mode,
                "pair_url": pair_portal_url(&pairing.pairing_token),
                "remote": status.as_ref().map(remote_status_json),
            }));
        }
        print_success_summary(
            "Fresh pair token ready",
            &[
                ("Device ID", pairing.device_id.clone()),
                ("Pair token", pairing.pairing_token.clone()),
                ("Pair URL", pair_portal_url(&pairing.pairing_token)),
            ],
        );
        render_pair_session(&pairing, &runtime.config, status.as_ref())?;
        return Ok(());
    }

    if let Some(token) = token {
        runtime.storage.update_pairing_token(token.trim().to_string())?;
        let pairing = runtime.pairing()?;
        let status = register_pairing_device(runtime, &pairing).await.ok();
        if runtime.is_json() {
            return runtime.emit_json(&serde_json::json!({
                "ok": true,
                "device_id": pairing.device_id,
                "pair_token": pairing.pairing_token,
                "transport_mode": runtime.config.transport_mode,
                "pair_url": pair_portal_url(&pairing.pairing_token),
                "remote": status.as_ref().map(remote_status_json),
            }));
        }
        print_success_summary(
            "Pair token updated",
            &[
                ("Device ID", pairing.device_id.clone()),
                ("Pair token", pairing.pairing_token.clone()),
                ("Pair URL", pair_portal_url(&pairing.pairing_token)),
            ],
        );
        render_pair_session(&pairing, &runtime.config, status.as_ref())?;
        return Ok(());
    }

    let pairing = runtime.pairing()?;
    let status = register_pairing_device(runtime, &pairing).await.ok();
    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "device_id": pairing.device_id,
            "pair_token": pairing.pairing_token,
            "transport_mode": runtime.config.transport_mode,
            "pair_url": pair_portal_url(&pairing.pairing_token),
            "remote": status.as_ref().map(remote_status_json),
        }));
    }

    if open {
        open_pair_portal(&pairing.pairing_token)?;
        println!("{} opened your pair page in the browser.", success_tag());
    }

    render_pair_session(&pairing, &runtime.config, status.as_ref())?;

    if has_interactive_terminal() && !open {
        maybe_prompt_pair_action(runtime).await?;
    }

    Ok(())
}

async fn run_send_command(runtime: &CliRuntime, paths: Vec<String>) -> AppResult<()> {
    let started_at = Instant::now();
    let mut imported = Vec::new();
    for path in paths {
        let mut next = runtime
            .storage
            .import_paths(ImportPathPayload {
                paths: vec![path],
                source_kind: None,
            })
            .await?;
        imported.append(&mut next);
    }

    if imported.is_empty() {
        return Err(anyhow!("No files were imported.").into());
    }

    let relay = sync_items_to_relay(runtime, &imported).await?;

    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "item_count": imported.len(),
            "items": imported.iter().map(item_json_summary).collect::<Vec<_>>(),
            "relay": relay,
        }));
    }

    let total_bytes = imported
        .iter()
        .filter_map(|item| item.size_bytes)
        .map(|value| value.max(0) as u64)
        .sum::<u64>();

    print_success_summary(
        "Items shared",
        &[
            ("Queued items", imported.len().to_string()),
            ("Approx bytes", format_bytes(total_bytes)),
            ("Finished in", format_duration_compact(started_at.elapsed())),
            (
                "Remote session",
                if relay.remote_paired { "live" } else { "local only" }.to_string(),
            ),
        ],
    );

    for item in imported {
        println!(
            "  {} {:<7} {}",
            bullet(),
            plain_item_type(&item.item_type),
            item_display_name(&item)
        );
    }
    Ok(())
}

async fn run_send_text_command(runtime: &CliRuntime, text: Option<String>) -> AppResult<()> {
    let text = resolve_send_text_payload(text)?;
    let started_at = Instant::now();
    let line_count = text.lines().count().max(1);
    let char_count = text.chars().count();
    let item = runtime.storage.import_text(text, None).await?;
    let relay = sync_items_to_relay(runtime, std::slice::from_ref(&item)).await?;

    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "item": item_json_summary(&item),
            "relay": relay,
        }));
    }

    print_success_summary(
        "Text shared",
        &[
            ("Item ID", item.id.clone()),
            ("Lines", line_count.to_string()),
            ("Characters", char_count.to_string()),
            ("Finished in", format_duration_compact(started_at.elapsed())),
        ],
    );
    Ok(())
}

async fn run_send_bundle_command(
    runtime: &CliRuntime,
    title: Option<String>,
    source_label: Option<String>,
    source_url: Option<String>,
    transcript_file: Option<String>,
    files: Vec<String>,
    attachments: Vec<String>,
) -> AppResult<()> {
    let transcript_markdown = resolve_bundle_transcript_markdown(transcript_file)?;
    let started_at = Instant::now();
    let file_sources = resolve_bundle_sources(files)?;
    let attachment_sources = resolve_bundle_sources(attachments)?;
    let referenced_count = file_sources.len();
    let attachment_count = attachment_sources.len();
    let line_count = transcript_markdown.lines().count().max(1);
    let char_count = transcript_markdown.chars().count();

    let item = runtime
        .storage
        .import_shared_conversation_bundle(ShareBundleOrigin::Cli, ImportConversationBundlePayload {
            title,
            transcript_markdown,
            source_label,
            source_url,
            files: file_sources,
            attachments: attachment_sources,
        })
        .await?;
    let relay = sync_items_to_relay(runtime, std::slice::from_ref(&item)).await?;

    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "item": item_json_summary(&item),
            "referenced_files": referenced_count,
            "attachments": attachment_count,
            "relay": relay,
        }));
    }

    print_success_summary(
        "Conversation bundle shared",
        &[
            ("Item ID", item.id.clone()),
            ("Transcript lines", line_count.to_string()),
            ("Characters", char_count.to_string()),
            ("Referenced files", referenced_count.to_string()),
            ("Attachments", attachment_count.to_string()),
            ("Finished in", format_duration_compact(started_at.elapsed())),
        ],
    );
    Ok(())
}

async fn run_list_command(runtime: &CliRuntime) -> AppResult<()> {
    let items = runtime.storage.list_items().await?;
    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "items": items,
        }));
    }

    print_banner("Your local Dropply cache.");

    if items.is_empty() {
        println!("{} Nothing is cached locally yet.", info_tag());
        println!("{} Try 'dropply-cli send <file>', 'dropply-cli send-text', or 'dropply-cli pull'.", info_tag());
        return Ok(());
    }

    let text_count = items
        .iter()
        .filter(|item| matches!(item.item_type, ItemType::Text))
        .count();
    let image_count = items
        .iter()
        .filter(|item| matches!(item.item_type, ItemType::Image))
        .count();
    let file_count = items
        .iter()
        .filter(|item| matches!(item.item_type, ItemType::File))
        .count();

    print_block_header("Cache summary");
    print_kv("Items", items.len().to_string());
    print_kv("Text", text_count.to_string());
    print_kv("Images", image_count.to_string());
    print_kv("Files", file_count.to_string());

    print_block_header("Items");
    for item in items {
        let label = item
            .name
            .clone()
            .or_else(|| item.text_preview.as_deref().map(|text| shorten_line(text, 48)))
            .unwrap_or_else(|| item.id.clone());
        println!(
            "{}  {:<7}  {:<16}  {}",
            dim(&short_id(&item.id)),
            style_item_type(&item.item_type),
            format_timestamp(&item.updated_at),
            label
        );
    }

    Ok(())
}

async fn run_pull_command(runtime: &CliRuntime) -> AppResult<()> {
    let started_at = Instant::now();
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
    let mut imported_items = Vec::new();
    let mut deleted_items = Vec::new();

    for item in remote.items {
        if item.deleted.unwrap_or(false) {
            if existing_ids.remove(&item.id) {
                runtime.storage.delete_item(&item.id).await?;
                deleted_count += 1;
                deleted_items.push(item.id);
            }
            continue;
        }

        if existing_ids.contains(&item.id) {
            continue;
        }

        let imported = if matches!(item.item_type, ItemType::Text) || item.bytes_b64.is_some() {
            runtime.storage.import_relay_item(item).await?
        } else {
            pull_relay_blob_to_storage(runtime, &pairing, item).await?
        };
        imported_count += 1;
        imported_items.push(item_json_summary(&imported));
    }

    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "imported_count": imported_count,
            "deleted_count": deleted_count,
            "imported_items": imported_items,
            "deleted_items": deleted_items,
        }));
    }

    if imported_count == 0 && deleted_count == 0 {
        println!("{} Nothing new to pull. Your CLI cache is already current.", info_tag());
        println!("{} Leave 'dropply-cli status --watch' running if you want a live session view.", info_tag());
        return Ok(());
    }

    print_success_summary(
        "Pull complete",
        &[
            ("Imported", imported_count.to_string()),
            ("Removed", deleted_count.to_string()),
            ("Finished in", format_duration_compact(started_at.elapsed())),
        ],
    );
    Ok(())
}

async fn run_status_command(runtime: &CliRuntime, watch: bool) -> AppResult<()> {
    if runtime.is_json() {
        if watch {
            return Err(anyhow!("status --watch is only available in pretty mode").into());
        }
        let snapshot = collect_status_snapshot(runtime).await?;
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "device_id": snapshot.pairing.device_id,
            "pair_token": snapshot.pairing.pairing_token,
            "transport_mode": snapshot.transport_mode,
            "api_base": snapshot.api_base_url,
            "local_item_count": snapshot.local_items.len(),
            "data_dir": snapshot.data_dir,
            "storage_fallback": snapshot.used_storage_fallback,
            "pair_url": pair_portal_url(&snapshot.pairing.pairing_token),
            "remote": snapshot.remote.as_ref().map(remote_status_json),
        }));
    }

    if !watch {
        let snapshot = collect_status_snapshot(runtime).await?;
        render_status_snapshot(&snapshot);
        return Ok(());
    }

    loop {
        let snapshot = collect_status_snapshot(runtime).await?;
        clear_screen();
        render_status_snapshot(&snapshot);
        println!();
        println!(
            "{} watching session updates. Press Ctrl+C to exit.",
            dim("live")
        );

        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(STATUS_WATCH_INTERVAL) => {}
        }
    }

    Ok(())
}

async fn run_transport_command(runtime: &CliRuntime, mode: Option<TransportMode>) -> AppResult<()> {
    if let Some(mode) = mode {
        let mut next_config = runtime.config.clone();
        next_config.transport_mode = mode;
        save_cli_config(runtime.config_path(), &next_config)?;
        if runtime.is_json() {
            return runtime.emit_json(&serde_json::json!({
                "ok": true,
                "transport_mode": mode,
            }));
        }

        print_success_summary(
            "Transport preference updated",
            &[
                ("CLI mode", mode.to_string()),
                (
                    "Note",
                    "This advertises the preferred path for this device, even though CLI orchestration still uses the hosted relay session path today."
                        .to_string(),
                ),
            ],
        );
        return Ok(());
    }

    if runtime.is_json() {
        return runtime.emit_json(&serde_json::json!({
            "ok": true,
            "transport_mode": runtime.config.transport_mode,
        }));
    }

    print_banner("Transport preference for this CLI device.");
    print_block_header("Transport");
    print_kv("CLI mode", runtime.config.transport_mode.to_string());
    println!("{} Use 'dropply-cli transport --mode auto|p2p|relay' to change it.", info_tag());
    Ok(())
}

async fn collect_status_snapshot(runtime: &CliRuntime) -> AppResult<StatusSnapshot> {
    let pairing = runtime.pairing()?;
    let local_items = runtime.storage.list_items().await?;
    let remote = register_pairing_device(runtime, &pairing).await.ok();
    Ok(StatusSnapshot {
        pairing,
        local_items,
        remote,
        transport_mode: runtime.config.transport_mode,
        api_base_url: runtime.api_base_url.clone(),
        data_dir: runtime.storage.base_dir().display().to_string(),
        used_storage_fallback: runtime.used_storage_fallback,
    })
}

async fn sync_items_to_relay(runtime: &CliRuntime, items: &[ItemPayload]) -> AppResult<RelaySyncSummary> {
    let pairing = runtime.pairing()?;
    let status = register_pairing_device(runtime, &pairing).await?;
    if status.paired_device_count < 2 {
        if !runtime.is_json() {
            println!("{} no remote device is paired yet, so this is saved locally for now.", info_tag());
            println!("{} Open Dropply on your phone or browser, then run 'dropply-cli pull' there later.", info_tag());
        }
        return Ok(RelaySyncSummary {
            remote_paired: false,
            snapshot_pushed: false,
        });
    }

    for item in items
        .iter()
        .filter(|item| matches!(item.item_type, ItemType::Image | ItemType::File))
    {
        push_relay_blob(runtime, &pairing, item).await?;
    }

    push_relay_snapshot(runtime, &pairing).await?;
    if !runtime.is_json() {
        println!("{} relay session updated.", success_tag());
    }
    Ok(RelaySyncSummary {
        remote_paired: true,
        snapshot_pushed: true,
    })
}

async fn register_pairing_device(runtime: &CliRuntime, pairing: &PairingInfo) -> AppResult<PairStatus> {
    #[derive(Serialize)]
    struct PairRegisterRequest<'a> {
        token: &'a str,
        #[serde(rename = "deviceId")]
        device_id: &'a str,
        #[serde(rename = "deviceType")]
        device_type: &'a str,
        label: &'a str,
        #[serde(rename = "transportPreference", skip_serializing_if = "Option::is_none")]
        transport_preference: Option<&'a str>,
    }

    post_json(
        &runtime.client,
        format!("{}/v1/pair/register", runtime.api_base_url),
        &PairRegisterRequest {
            token: &pairing.pairing_token,
            device_id: &pairing.device_id,
            device_type: CLI_DEVICE_TYPE,
            label: CLI_DEVICE_LABEL,
            transport_preference: runtime.config.transport_mode.pair_preference(),
        },
    )
    .await
}

async fn fetch_relay_pull(runtime: &CliRuntime, pairing: &PairingInfo) -> AppResult<RelayPullResponse> {
    get_json(
        &runtime.client,
        format!(
            "{}/v1/relay/pull?token={}&deviceId={}",
            runtime.api_base_url, pairing.pairing_token, pairing.device_id
        ),
    )
    .await
}

async fn push_relay_snapshot(runtime: &CliRuntime, pairing: &PairingInfo) -> AppResult<()> {
    #[derive(Serialize)]
    struct RelayPushRequest<'a> {
        token: &'a str,
        #[serde(rename = "deviceId")]
        device_id: &'a str,
        items: Vec<RelayItemPayload>,
    }

    let manifest = runtime.storage.export_pair_manifest().await?;
    let snapshot = build_relay_snapshot(pairing, manifest)?;

    let _: serde_json::Value = post_json(
        &runtime.client,
        format!("{}/v1/relay/push", runtime.api_base_url),
        &RelayPushRequest {
            token: &pairing.pairing_token,
            device_id: &pairing.device_id,
            items: snapshot,
        },
    )
    .await?;

    Ok(())
}

async fn push_relay_blob(runtime: &CliRuntime, pairing: &PairingInfo, item: &ItemPayload) -> AppResult<()> {
    let size_bytes = item.size_bytes.unwrap_or(0).max(0) as usize;
    let chunk_bytes = resolve_relay_blob_chunk_bytes(size_bytes, item.mime_type.as_deref());
    let worker_count = resolve_relay_blob_parallel_workers(size_bytes, item.mime_type.as_deref());
    let blob = runtime.storage.export_relay_blob(&item.id, chunk_bytes).await?;
    if blob.chunks.is_empty() {
        return Ok(());
    }
    let blob = Arc::new(blob);

    let file_label = item_display_name(item);
    let uploaded = fetch_relay_blob_upload_status(runtime, pairing, item, &blob)
        .await
        .ok()
        .filter(|status| status.resumable)
        .map(|status| expand_received_ranges(&status.received_ranges))
        .unwrap_or_default();

    let completed_chunks = uploaded.len();
    let completed_bytes = uploaded_byte_count(&uploaded, blob.size_bytes.max(0) as u64, chunk_bytes);
    let meter = TransferMeter::new(
        "send",
        file_label.clone(),
        blob.size_bytes.max(0) as u64,
        blob.chunks.len(),
    );

    if runtime.can_render_pretty() && completed_chunks > 0 && completed_chunks < blob.chunks.len() {
        meter.render("resuming", completed_bytes, completed_chunks, None);
    }

    if completed_chunks >= blob.chunks.len() {
        if runtime.can_render_pretty() {
            meter.render("complete", blob.size_bytes.max(0) as u64, blob.chunks.len(), None);
        }
        return Ok(());
    }

    let pending_chunk_indices = Arc::new(
        (0..blob.chunks.len())
            .filter(|chunk_index| !uploaded.contains(chunk_index))
            .collect::<Vec<_>>(),
    );
    let next_pending_index = Arc::new(AtomicUsize::new(0));
    let progress = Arc::new(Mutex::new(ChunkProgressState {
        completed_chunks,
        completed_bytes,
    }));

    stream::iter(0..worker_count)
        .map(Ok::<usize, anyhow::Error>)
        .try_for_each_concurrent(worker_count, |_| {
            let pending_chunk_indices = Arc::clone(&pending_chunk_indices);
            let next_pending_index = Arc::clone(&next_pending_index);
            let progress = Arc::clone(&progress);
            let blob = Arc::clone(&blob);
            let file_label = file_label.clone();
            let meter = meter.clone();
            async move {
                loop {
                    let queue_index = next_pending_index.fetch_add(1, Ordering::SeqCst);
                    if queue_index >= pending_chunk_indices.len() {
                        break;
                    }

                    let chunk_index = pending_chunk_indices[queue_index];
                    let params = relay_blob_query_params(
                        pairing,
                        item,
                        &blob,
                        chunk_index,
                    );
                    let chunk_bytes_len =
                        chunk_byte_length(chunk_index, blob.size_bytes.max(0) as u64, chunk_bytes);
                    let body = BASE64
                        .decode(blob.chunks[chunk_index].as_bytes())
                        .context("invalid relay blob chunk encoding")?;

                    post_binary_with_retry(
                        &runtime.client,
                        format!("{}/v1/relay/blob/push-binary?{params}", runtime.api_base_url),
                        body,
                        format!("Relay blob push failed for {}.", file_label),
                        |retry_event| {
                            if runtime.can_render_pretty() {
                                if let Ok(state) = progress.lock() {
                                    meter.render(
                                        "retrying",
                                        state.completed_bytes,
                                        state.completed_chunks,
                                        Some(retry_event),
                                    );
                                }
                            }
                        },
                    )
                    .await?;

                    if let Ok(mut state) = progress.lock() {
                        state.completed_chunks += 1;
                        state.completed_bytes += chunk_bytes_len;
                        if runtime.can_render_pretty() {
                            meter.render("uploading", state.completed_bytes, state.completed_chunks, None);
                        }
                    }
                }

                Ok(())
            }
        })
        .await?;

    if runtime.can_render_pretty() {
        if let Ok(state) = progress.lock() {
            meter.render("complete", state.completed_bytes, state.completed_chunks, None);
        }
    }

    Ok(())
}

async fn fetch_relay_blob_upload_status(
    runtime: &CliRuntime,
    pairing: &PairingInfo,
    item: &ItemPayload,
    blob: &RelayBlobPayload,
) -> AppResult<RelayBlobUploadStatus> {
    let mut url = format!(
        "{}/v1/relay/blob/status?token={}&deviceId={}&itemId={}&updated_at={}&totalChunks={}",
        runtime.api_base_url,
        pairing.pairing_token,
        pairing.device_id,
        item.id,
        blob.updated_at.to_rfc3339(),
        blob.chunks.len()
    );

    if let Some(size_bytes) = item.size_bytes.or(Some(blob.size_bytes)) {
        url.push_str(&format!("&size_bytes={size_bytes}"));
    }

    if let Some(sha256) = blob.sha256.clone().or_else(|| item.sha256.clone()) {
        url.push_str(&format!("&sha256={sha256}"));
    }

    get_json(&runtime.client, url).await
}

async fn pull_relay_blob_to_storage(
    runtime: &CliRuntime,
    pairing: &PairingInfo,
    item: RelayItemPayload,
) -> AppResult<ItemPayload> {
    let file_label = item
        .name
        .clone()
        .unwrap_or_else(|| item.id.clone());
    let staged_dir = runtime.storage.base_dir().join("staging");
    std::fs::create_dir_all(&staged_dir)?;
    let staged_path = staged_dir.join(stage_file_name(&item));
    let _ = tokio::fs::remove_file(&staged_path).await;

    let first_chunk = fetch_relay_blob_chunk_json(runtime, pairing, &item.id, 0).await?;
    let total_chunks = first_chunk.total_chunks.max(1);
    let first_bytes = BASE64
        .decode(first_chunk.bytes_b64.as_bytes())
        .context("invalid relay chunk encoding")?;
    let total_bytes = item.size_bytes.unwrap_or(first_bytes.len() as i64).max(0) as u64;
    let worker_count = resolve_relay_blob_parallel_workers(
        total_bytes as usize,
        item.mime_type.as_deref(),
    );
    let meter = TransferMeter::new("pull", file_label.clone(), total_bytes, total_chunks);
    let progress = Arc::new(Mutex::new(ChunkProgressState {
        completed_chunks: 1,
        completed_bytes: first_bytes.len() as u64,
    }));

    if runtime.can_render_pretty() {
        meter.render("downloading", first_bytes.len() as u64, 1, None);
    }

    let chunk_buffers = Arc::new(Mutex::new(vec![None; total_chunks]));
    if let Ok(mut buffer_guard) = chunk_buffers.lock() {
        buffer_guard[0] = Some(first_bytes);
    }

    if total_chunks > 1 {
        let pending_chunk_indices = Arc::new((1..total_chunks).collect::<Vec<_>>());
        let next_pending_index = Arc::new(AtomicUsize::new(0));
        let item_id = item.id.clone();

        stream::iter(0..worker_count)
            .map(Ok::<usize, anyhow::Error>)
            .try_for_each_concurrent(worker_count, |_| {
                let chunk_buffers = Arc::clone(&chunk_buffers);
                let pending_chunk_indices = Arc::clone(&pending_chunk_indices);
                let next_pending_index = Arc::clone(&next_pending_index);
                let progress = Arc::clone(&progress);
                let item_id = item_id.clone();
                let file_label = file_label.clone();
                let meter = meter.clone();
                async move {
                    loop {
                        let queue_index = next_pending_index.fetch_add(1, Ordering::SeqCst);
                        if queue_index >= pending_chunk_indices.len() {
                            break;
                        }

                        let chunk_index = pending_chunk_indices[queue_index];
                        let bytes = get_binary_with_retry(
                            &runtime.client,
                            format!(
                                "{}/v1/relay/blob/chunk-binary?token={}&deviceId={}&itemId={}&chunkIndex={}",
                                runtime.api_base_url,
                                pairing.pairing_token,
                                pairing.device_id,
                                item_id,
                                chunk_index
                            ),
                            format!("Relay chunk download failed for {}.", file_label),
                            |retry_event| {
                                if runtime.can_render_pretty() {
                                    if let Ok(state) = progress.lock() {
                                        meter.render(
                                            "retrying",
                                            state.completed_bytes,
                                            state.completed_chunks,
                                            Some(retry_event),
                                        );
                                    }
                                }
                            },
                        )
                        .await?;

                        let chunk_len = bytes.len() as u64;
                        if let Ok(mut buffer_guard) = chunk_buffers.lock() {
                            buffer_guard[chunk_index] = Some(bytes);
                        }
                        if let Ok(mut state) = progress.lock() {
                            state.completed_chunks += 1;
                            state.completed_bytes += chunk_len;
                            if runtime.can_render_pretty() {
                                meter.render(
                                    "downloading",
                                    state.completed_bytes,
                                    state.completed_chunks,
                                    None,
                                );
                            }
                        }
                    }

                    Ok(())
                }
            })
            .await?;
    }

    if runtime.can_render_pretty() {
        if let Ok(state) = progress.lock() {
            meter.render("complete", state.completed_bytes, state.completed_chunks, None);
        }
    }

    let mut staged_file = tokio::fs::File::create(&staged_path).await?;
    let mut buffer_guard = chunk_buffers
        .lock()
        .map_err(|_| anyhow!("relay chunk buffer lock was poisoned"))?;
    for chunk_index in 0..total_chunks {
        let bytes = buffer_guard[chunk_index]
            .take()
            .ok_or_else(|| anyhow!("missing relay chunk {}", chunk_index))?;
        tokio::io::AsyncWriteExt::write_all(&mut staged_file, &bytes).await?;
    }
    tokio::io::AsyncWriteExt::flush(&mut staged_file).await?;
    drop(staged_file);
    drop(buffer_guard);

    match runtime.storage.import_staged_relay_item(item, &staged_path).await {
        Ok(imported) => Ok(imported),
        Err(error) => {
            let _ = tokio::fs::remove_file(&staged_path).await;
            Err(error)
        }
    }
}

async fn fetch_relay_blob_chunk_json(
    runtime: &CliRuntime,
    pairing: &PairingInfo,
    item_id: &str,
    chunk_index: usize,
) -> AppResult<RelayBlobChunkResponse> {
    get_json(
        &runtime.client,
        format!(
            "{}/v1/relay/blob/chunk?token={}&deviceId={}&itemId={}&chunkIndex={}",
            runtime.api_base_url, pairing.pairing_token, pairing.device_id, item_id, chunk_index
        ),
    )
    .await
}

fn build_relay_snapshot(pairing: &PairingInfo, items: Vec<RelayItemPayload>) -> AppResult<Vec<RelayItemPayload>> {
    let mut remaining_inline_budget = MAX_RELAY_SNAPSHOT_INLINE_B64_CHARS;
    let mut snapshot = Vec::new();

    let mut sorted = items;
    sorted.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    for item in sorted {
        let mut candidate = if item.bytes_b64.is_some() && !can_inline_relay_bytes(&item) {
            let mut trimmed = item.clone();
            trimmed.bytes_b64 = None;
            trimmed
        } else {
            item.clone()
        };

        if let Some(bytes_b64) = candidate.bytes_b64.as_ref() {
            if bytes_b64.len() > remaining_inline_budget {
                candidate.bytes_b64 = None;
            }
        }

        let next_snapshot = {
            let mut next = snapshot.clone();
            next.push(candidate.clone());
            next
        };

        if relay_snapshot_payload_len(&pairing.pairing_token, &pairing.device_id, &next_snapshot)?
            > MAX_RELAY_SNAPSHOT_JSON_CHARS
        {
            if item.bytes_b64.is_none() {
                continue;
            }

            candidate.bytes_b64 = None;
            let trimmed_snapshot = {
                let mut next = snapshot.clone();
                next.push(candidate.clone());
                next
            };

            if relay_snapshot_payload_len(
                &pairing.pairing_token,
                &pairing.device_id,
                &trimmed_snapshot,
            )? > MAX_RELAY_SNAPSHOT_JSON_CHARS
            {
                continue;
            }
        }

        if let Some(bytes_b64) = candidate.bytes_b64.as_ref() {
            remaining_inline_budget = remaining_inline_budget.saturating_sub(bytes_b64.len());
        }

        snapshot.push(candidate);
    }

    Ok(snapshot)
}

fn relay_snapshot_payload_len(token: &str, device_id: &str, items: &[RelayItemPayload]) -> AppResult<usize> {
    Ok(
        serde_json::to_string(&serde_json::json!({
            "token": token,
            "deviceId": device_id,
            "items": items,
        }))?
        .len(),
    )
}

fn can_inline_relay_bytes(item: &RelayItemPayload) -> bool {
    matches!(item.item_type, ItemType::Image)
        && item
            .mime_type
            .as_deref()
            .map(|mime| mime.starts_with("image/"))
            .unwrap_or(true)
}

fn expand_received_ranges(ranges: &[RelayBlobStatusRange]) -> HashSet<usize> {
    let mut uploaded = HashSet::new();
    for range in ranges {
        for chunk_index in range.start..=range.end {
            uploaded.insert(chunk_index);
        }
    }
    uploaded
}

fn resolve_relay_blob_chunk_bytes(size_bytes: usize, mime_type: Option<&str>) -> usize {
    if mime_type.map(|mime| mime.starts_with("video/")).unwrap_or(false) || size_bytes >= 32 * 1024 * 1024 {
        return LARGE_RELAY_BLOB_CHUNK_BYTES;
    }

    if size_bytes >= 8 * 1024 * 1024 {
        return MEDIUM_RELAY_BLOB_CHUNK_BYTES;
    }

    BASE_RELAY_BLOB_CHUNK_BYTES
}

fn resolve_relay_blob_parallel_workers(size_bytes: usize, mime_type: Option<&str>) -> usize {
    if mime_type.map(|mime| mime.starts_with("video/")).unwrap_or(false) || size_bytes >= 32 * 1024 * 1024 {
        return LARGE_RELAY_BLOB_PARALLEL_WORKERS;
    }

    if size_bytes >= 8 * 1024 * 1024 {
        return MEDIUM_RELAY_BLOB_PARALLEL_WORKERS;
    }

    SMALL_RELAY_BLOB_PARALLEL_WORKERS
}

fn chunk_byte_length(chunk_index: usize, total_bytes: u64, chunk_bytes: usize) -> u64 {
    let start = chunk_index.saturating_mul(chunk_bytes) as u64;
    let end = (start + chunk_bytes as u64).min(total_bytes);
    end.saturating_sub(start)
}

fn uploaded_byte_count(uploaded_chunks: &HashSet<usize>, total_bytes: u64, chunk_bytes: usize) -> u64 {
    uploaded_chunks
        .iter()
        .map(|chunk_index| chunk_byte_length(*chunk_index, total_bytes, chunk_bytes))
        .sum()
}

fn relay_blob_query_params(
    pairing: &PairingInfo,
    item: &ItemPayload,
    blob: &RelayBlobPayload,
    chunk_index: usize,
) -> String {
    let mut params = vec![
        format!("token={}", pairing.pairing_token),
        format!("deviceId={}", pairing.device_id),
        format!("itemId={}", item.id),
        format!("updated_at={}", blob.updated_at.to_rfc3339()),
        format!(
            "mime_type={}",
            blob.mime_type
                .clone()
                .or_else(|| item.mime_type.clone())
                .unwrap_or_else(|| "application/octet-stream".to_string())
        ),
        format!("size_bytes={}", blob.size_bytes),
        format!("totalChunks={}", blob.chunks.len()),
        format!("chunkIndex={chunk_index}"),
    ];

    if let Some(sha256) = blob.sha256.clone().or_else(|| item.sha256.clone()) {
        params.push(format!("sha256={sha256}"));
    }

    params.join("&")
}

async fn get_json<T: DeserializeOwned>(client: &Client, url: String) -> AppResult<T> {
    let response = client.get(url).send().await.context("request failed")?;
    parse_json_response(response).await
}

async fn post_json<T: DeserializeOwned, B: Serialize>(
    client: &Client,
    url: String,
    body: &B,
) -> AppResult<T> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .context("request failed")?;
    parse_json_response(response).await
}

async fn parse_json_response<T: DeserializeOwned>(response: Response) -> AppResult<T> {
    if response.status().is_success() {
        return response
            .json::<T>()
            .await
            .context("invalid response payload")
            .map_err(Into::into);
    }

    let status = response.status();
    let text = response.text().await.unwrap_or_else(|_| String::new());
    let message = parse_error_message(status, &text);
    Err(anyhow!(message).into())
}

async fn post_binary_with_retry<F>(
    client: &Client,
    url: String,
    body: Vec<u8>,
    fallback_message: String,
    mut on_retry: F,
) -> AppResult<Response>
where
    F: FnMut(RetryEvent),
{
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=MAX_RELAY_REQUEST_RETRIES {
        let response = client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .body(body.clone())
            .send()
            .await;

        match response {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let status = response.status();
                if attempt < MAX_RELAY_REQUEST_RETRIES && is_retryable_relay_status(status) {
                    let delay_ms = compute_retry_delay_ms(attempt, parse_retry_after_ms(response.headers()));
                    on_retry(RetryEvent {
                        attempt: attempt + 1,
                        total_attempts: MAX_RELAY_REQUEST_RETRIES,
                    });
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }

                let message = read_error_message(response, &fallback_message).await;
                last_error = Some(anyhow!(message));
                break;
            }
            Err(error) => {
                last_error = Some(anyhow!(error));
                if attempt < MAX_RELAY_REQUEST_RETRIES {
                    let delay_ms = compute_retry_delay_ms(attempt, None);
                    on_retry(RetryEvent {
                        attempt: attempt + 1,
                        total_attempts: MAX_RELAY_REQUEST_RETRIES,
                    });
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!(fallback_message)).into())
}

async fn get_binary_with_retry<F>(
    client: &Client,
    url: String,
    fallback_message: String,
    mut on_retry: F,
) -> AppResult<Vec<u8>>
where
    F: FnMut(RetryEvent),
{
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=MAX_RELAY_REQUEST_RETRIES {
        let response = client.get(&url).send().await;

        match response {
            Ok(response) if response.status().is_success() => {
                let bytes = response.bytes().await.context("invalid binary relay response")?;
                return Ok(bytes.to_vec());
            }
            Ok(response) => {
                let status = response.status();
                if attempt < MAX_RELAY_REQUEST_RETRIES && is_retryable_relay_status(status) {
                    let delay_ms = compute_retry_delay_ms(attempt, parse_retry_after_ms(response.headers()));
                    on_retry(RetryEvent {
                        attempt: attempt + 1,
                        total_attempts: MAX_RELAY_REQUEST_RETRIES,
                    });
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }

                let message = read_error_message(response, &fallback_message).await;
                last_error = Some(anyhow!(message));
                break;
            }
            Err(error) => {
                last_error = Some(anyhow!(error));
                if attempt < MAX_RELAY_REQUEST_RETRIES {
                    let delay_ms = compute_retry_delay_ms(attempt, None);
                    on_retry(RetryEvent {
                        attempt: attempt + 1,
                        total_attempts: MAX_RELAY_REQUEST_RETRIES,
                    });
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!(fallback_message)).into())
}

async fn read_error_message(response: Response, fallback_message: &str) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_else(|_| String::new());
    if text.trim().is_empty() {
        return format!("{} (HTTP {})", fallback_message, status.as_u16());
    }
    parse_error_message(status, &text)
}

fn parse_error_message(status: StatusCode, text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(message) = value.get("error").and_then(|value| value.as_str()) {
            return message.to_string();
        }
    }

    if !text.trim().is_empty() {
        return format!("HTTP {}: {}", status.as_u16(), text.trim());
    }

    format!("HTTP {}", status.as_u16())
}

fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000))
}

fn compute_retry_delay_ms(attempt: usize, retry_after_ms: Option<u64>) -> u64 {
    if let Some(retry_after_ms) = retry_after_ms {
        return retry_after_ms.min(RELAY_RETRY_MAX_DELAY_MS);
    }

    let multiplier = 1u64 << attempt.saturating_sub(1).min(5);
    (RELAY_RETRY_BASE_DELAY_MS.saturating_mul(multiplier)).min(RELAY_RETRY_MAX_DELAY_MS)
}

fn is_retryable_relay_status(status: StatusCode) -> bool {
    matches!(
        status.as_u16(),
        408 | 425 | 429 | 500 | 502 | 503 | 504
    )
}

fn render_pair_session(pairing: &PairingInfo, config: &CliConfig, status: Option<&PairStatus>) -> AppResult<()> {
    let pair_url = pair_portal_url(&pairing.pairing_token);
    print_banner("Pair this CLI into the same Dropply session as your desktop or browser.");

    print_block_header("Session");
    print_kv("Device ID", &pairing.device_id);
    print_kv("Pair token", &pairing.pairing_token);
    print_kv("Transport", config.transport_mode.to_string());
    print_kv("Pair URL", &pair_url);

    if let Some(status) = status {
        print_block_header("Remote");
        print_kv(
            "Paired devices",
            status.paired_device_count.saturating_sub(1).to_string(),
        );
        print_kv("Remote items", status.item_count.to_string());
        print_kv("Linked", if status.paired { "yes" } else { "no" });

        if !status.devices.is_empty() {
            print_block_header("Devices");
            for device in &status.devices {
                let last_seen = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(device.last_seen_at)
                    .map(|value| value.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {} {} [{} / {} / seen {}]",
                    bullet(),
                    device.label,
                    device.device_type,
                    device.transport_preference,
                    last_seen
                );
            }
        }
    } else {
        print_block_header("Remote");
        println!("{} remote status unavailable right now.", warn_tag());
    }

    print_block_header("Scan");
    println!("{}", render_pair_qr(&pair_url)?);
    println!("{} Scan from the phone camera, or open the pair URL above.", dim("tip"));
    println!("{} Press Enter to keep this token, paste a new token, or type /open or /reset.", dim("tip"));
    Ok(())
}

fn render_status_snapshot(snapshot: &StatusSnapshot) {
    print_banner("Live view of your Dropply CLI device.");

    print_block_header("Device");
    print_kv("Device ID", &snapshot.pairing.device_id);
    print_kv("Pair token", &snapshot.pairing.pairing_token);
    print_kv("Pair URL", pair_portal_url(&snapshot.pairing.pairing_token));
    print_kv("Transport", snapshot.transport_mode.to_string());
    print_kv("API base", &snapshot.api_base_url);

    print_block_header("Local cache");
    print_kv("Items", snapshot.local_items.len().to_string());
    print_kv("Data dir", &snapshot.data_dir);
    print_kv(
        "Storage path",
        if snapshot.used_storage_fallback {
            "fallback profile"
        } else {
            "primary profile"
        },
    );

    if snapshot.transport_mode != TransportMode::Relay {
        println!(
            "{} CLI orchestration still uses the hosted relay session path today, even when the preferred transport label is {}.",
            info_tag(),
            snapshot.transport_mode
        );
    }

    if snapshot.used_storage_fallback {
        println!(
            "{} This machine fell back to a clean standalone CLI data directory because the older nested CLI cache path was not writable.",
            info_tag()
        );
    }

    print_block_header("Remote");
    if let Some(remote) = snapshot.remote.as_ref() {
        print_kv(
            "Paired devices",
            remote.paired_device_count.saturating_sub(1).to_string(),
        );
        print_kv("Remote items", remote.item_count.to_string());
        print_kv("Linked", if remote.paired { "yes" } else { "no" });
    } else {
        println!("{} remote status unavailable.", warn_tag());
    }
}

fn resolve_send_text_payload(text: Option<String>) -> AppResult<String> {
    if let Some(text) = text {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }

    if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        if buffer.trim().is_empty() {
            return Err(anyhow!("No text provided. Pass text arguments or pipe stdin.").into());
        }
        return Ok(buffer);
    }

    prompt_multiline_text()
}

fn resolve_bundle_transcript_markdown(transcript_file: Option<String>) -> AppResult<String> {
    if let Some(path) = transcript_file {
        let transcript = std::fs::read_to_string(&path)
            .with_context(|| format!("Unable to read transcript file at {path}"))?;
        if transcript.trim().is_empty() {
            return Err(anyhow!("Transcript file is empty.").into());
        }
        return Ok(transcript);
    }

    resolve_send_text_payload(None)
}

fn resolve_bundle_sources(paths: Vec<String>) -> AppResult<Vec<ConversationBundleSourcePayload>> {
    let cwd = env::current_dir().ok();
    let mut sources = Vec::with_capacity(paths.len());

    for raw_path in paths {
        let path = PathBuf::from(&raw_path);
        if !path.exists() {
            return Err(anyhow!("Bundle source '{}' does not exist.", raw_path).into());
        }
        if !path.is_file() {
            return Err(anyhow!("Bundle source '{}' is not a file.", raw_path).into());
        }

        let archive_path = derive_bundle_archive_path(&path, cwd.as_deref());
        sources.push(ConversationBundleSourcePayload {
            path: Some(path.to_string_lossy().to_string()),
            archive_path: Some(archive_path),
            name: None,
            mime_type: None,
            text_content: None,
            bytes_b64: None,
        });
    }

    Ok(sources)
}

fn derive_bundle_archive_path(path: &Path, cwd: Option<&std::path::Path>) -> String {
    let relative = cwd
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./").trim_start_matches('/');
    if trimmed.is_empty() {
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("item")
            .to_string()
    } else {
        trimmed.to_string()
    }
}

fn prompt_multiline_text() -> AppResult<String> {
    print_banner("Type or paste text to share.");
    println!("{} Finish with a blank line on its own.", dim("tip"));
    println!();

    let mut text = String::new();
    loop {
        let mut line = String::new();
        let read = io::stdin().read_line(&mut line)?;
        if read == 0 {
            break;
        }
        if line.trim().is_empty() && !text.trim().is_empty() {
            break;
        }
        text.push_str(&line);
    }

    if text.trim().is_empty() {
        return Err(anyhow!("No text entered.").into());
    }

    Ok(text)
}

async fn maybe_prompt_pair_action(runtime: &CliRuntime) -> AppResult<()> {
    if !has_interactive_terminal() {
        return Ok(());
    }

    print!("\npair> ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        return Ok(());
    }

    if input.eq_ignore_ascii_case("/open") {
        let pairing = runtime.pairing()?;
        open_pair_portal(&pairing.pairing_token)?;
        println!("{} opened the browser pair page.", success_tag());
        return Ok(());
    }

    if input.eq_ignore_ascii_case("/reset") {
        let pairing_token = runtime.storage.reset_pairing_token()?;
        let pairing = runtime.pairing()?;
        let status = register_pairing_device(runtime, &pairing).await.ok();
        print_success_summary(
            "Fresh pair token ready",
            &[
                ("Pair token", pairing_token),
                ("Pair URL", pair_portal_url(&pairing.pairing_token)),
            ],
        );
        render_pair_session(&pairing, &runtime.config, status.as_ref())?;
        return Ok(());
    }

    runtime.storage.update_pairing_token(input.to_string())?;
    let pairing = runtime.pairing()?;
    let status = register_pairing_device(runtime, &pairing).await.ok();
    print_success_summary(
        "Pair token updated",
        &[
            ("Pair token", pairing.pairing_token.clone()),
            ("Pair URL", pair_portal_url(&pairing.pairing_token)),
        ],
    );
    render_pair_session(&pairing, &runtime.config, status.as_ref())?;
    Ok(())
}

fn pair_portal_url(pairing_token: &str) -> String {
    format!("{DEFAULT_PAIR_PORTAL_URL}?token={pairing_token}")
}

fn open_pair_portal(pairing_token: &str) -> AppResult<()> {
    webbrowser::open(&pair_portal_url(pairing_token))
        .context("failed to open the browser pair page")
        .map_err(Into::into)
}

fn render_pair_qr(data: &str) -> AppResult<String> {
    let qr = QrCode::encode_text(data, QrCodeEcc::Medium)
        .map_err(|_| anyhow!("failed to encode terminal QR code"))?;
    let mut out = String::new();
    let border = 2;
    for y in -border..(qr.size() + border) {
        for x in -border..(qr.size() + border) {
            let dark = x >= 0 && y >= 0 && x < qr.size() && y < qr.size() && qr.get_module(x, y);
            out.push_str(if dark { "██" } else { "  " });
        }
        out.push('\n');
    }
    Ok(out)
}

fn item_json_summary(item: &ItemPayload) -> serde_json::Value {
    serde_json::json!({
        "id": item.id,
        "type": item.item_type,
        "name": item.name,
        "mime_type": item.mime_type,
        "size_bytes": item.size_bytes,
        "sha256": item.sha256,
        "updated_at": item.updated_at,
    })
}

fn remote_status_json(status: &PairStatus) -> serde_json::Value {
    serde_json::json!({
        "paired": status.paired,
        "paired_device_count": status.paired_device_count.saturating_sub(1),
        "item_count": status.item_count,
        "devices": status.devices.iter().map(|device| {
            serde_json::json!({
                "device_id": device.device_id,
                "device_type": device.device_type,
                "label": device.label,
                "transport_preference": device.transport_preference,
                "last_seen_at": device.last_seen_at,
            })
        }).collect::<Vec<_>>(),
    })
}

fn format_timestamp(timestamp: &DateTime<chrono::Utc>) -> String {
    timestamp.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
}

fn play_intro_animation() {
    if env::var_os("DROPPLY_NO_INTRO").is_some() || !io::stdout().is_terminal() {
        return;
    }

    let frames = [
        [
            "                    [ file ]",
            "",
            "",
            "",
            "                  .------------.",
            "                  |  Dropply   |",
            "                  '------------'",
            "",
            "             desktop -> phone -> browser",
        ],
        [
            "",
            "                    [ file ]",
            "",
            "",
            "                  .------------.",
            "                  |  Dropply   |",
            "                  '------------'",
            "",
            "             desktop -> phone -> browser",
        ],
        [
            "",
            "",
            "                    [ file ]",
            "",
            "                  .------------.",
            "                  |  Dropply   |",
            "                  '------------'",
            "",
            "             desktop -> phone -> browser",
        ],
        [
            "",
            "",
            "",
            "                    [ file ]",
            "                  .------------.",
            "                  |  Dropply   |",
            "                  '------------'",
            "",
            "             desktop -> phone -> browser",
        ],
        [
            "",
            "",
            "",
            "",
            "                  .------------.",
            "                  |  Dropply   |",
            "                  | sharing... |",
            "                  '------------'",
            "             desktop -> phone -> browser",
        ],
    ];

    for frame in frames {
        clear_screen();
        print_banner("Local-first handoff for your devices.");
        for line in frame {
            println!("{line}");
        }
        let _ = io::stdout().flush();
        std::thread::sleep(Duration::from_millis(85));
    }
    clear_screen();
}

fn clear_screen() {
    if io::stdout().is_terminal() {
        print!("\x1b[2J\x1b[H");
        let _ = io::stdout().flush();
    }
}

fn has_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn shorten_line(input: &str, max_chars: usize) -> String {
    let trimmed = input.replace('\n', " ");
    let mut chars = trimmed.chars();
    let candidate = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{candidate}...")
    } else {
        candidate
    }
}

fn use_color() -> bool {
    env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal()
}

fn paint(text: &str, code: &str) -> String {
    if use_color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn label(text: &str) -> String {
    paint(text, "1;37")
}

fn dim(text: &str) -> String {
    paint(text, "90")
}

fn success_tag() -> String {
    paint("[ok]", "1;32")
}

fn info_tag() -> String {
    paint("[info]", "1;36")
}

fn warn_tag() -> String {
    paint("[warn]", "1;33")
}

fn bullet() -> String {
    paint(">", "1;36")
}

fn print_banner(subtitle: &str) {
    let rule = paint(&"=".repeat(HEADER_WIDTH), "90");
    println!("{rule}");
    println!("{} {}", paint("Dropply CLI", "1;36"), dim(subtitle));
    println!("{rule}");
}

fn print_block_header(title: &str) {
    println!();
    let title_text = format!(" {title} ");
    let remaining = HEADER_WIDTH.saturating_sub(title_text.len());
    println!("{}{}", paint(&title_text, "1;36"), dim(&"-".repeat(remaining)));
}

fn print_command_row(command: &str, description: &str) {
    println!("  {:<42} {}", command, dim(description));
}

fn print_kv(key: &str, value: impl Display) {
    println!("{:18} {}", label(&format!("{key}:")), value);
}

fn print_success_summary(title: &str, rows: &[(&str, String)]) {
    println!("{} {}", success_tag(), title);
    for (key, value) in rows {
        print_kv(key, value);
    }
    println!();
}

fn progress_bar(percent: usize, width: usize) -> String {
    let filled = ((percent.min(100) as f64 / 100.0) * width as f64).round() as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled.min(width)),
        "-".repeat(width.saturating_sub(filled.min(width)))
    )
}

fn phase_badge(phase: &str) -> String {
    match phase {
        "resuming" => paint("resuming", "1;34"),
        "uploading" => paint("uploading", "1;36"),
        "downloading" => paint("downloading", "1;35"),
        "retrying" => paint("retrying", "1;33"),
        "complete" => paint("complete", "1;32"),
        other => other.to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn format_duration_compact(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    if total_seconds < 60 {
        return format!("{}s", total_seconds);
    }

    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m {seconds:02}s");
    }

    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    format!("{hours}h {remaining_minutes:02}m")
}

fn short_id(id: &str) -> String {
    if id.len() <= 10 {
        id.to_string()
    } else {
        format!("{}..{}", &id[..6], &id[id.len() - 4..])
    }
}

fn plain_item_type(item_type: &ItemType) -> &'static str {
    match item_type {
        ItemType::Text => "text",
        ItemType::Image => "image",
        ItemType::File => "file",
    }
}

fn style_item_type(item_type: &ItemType) -> String {
    match item_type {
        ItemType::Text => paint("text", "1;35"),
        ItemType::Image => paint("image", "1;34"),
        ItemType::File => paint("file", "1;32"),
    }
}

fn maybe_wait_for_explorer_close() {
    if launched_from_explorer() {
        print!("\nPress Enter to close...");
        let _ = io::stdout().flush();
        let mut buffer = String::new();
        let _ = io::stdin().read_line(&mut buffer);
    }
}

#[cfg(not(windows))]
fn launched_from_explorer() -> bool {
    false
}

#[cfg(windows)]
fn launched_from_explorer() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..std::mem::zeroed()
        };
        let current_pid = std::process::id();
        let mut parent_pid = 0u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == current_pid {
                    parent_pid = entry.th32ParentProcessID;
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        let mut parent_name = String::new();
        if parent_pid != 0 {
            let mut parent_entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..std::mem::zeroed()
            };
            if Process32FirstW(snapshot, &mut parent_entry) != 0 {
                loop {
                    if parent_entry.th32ProcessID == parent_pid {
                        let len = parent_entry
                            .szExeFile
                            .iter()
                            .position(|value| *value == 0)
                            .unwrap_or(parent_entry.szExeFile.len());
                        parent_name = String::from_utf16_lossy(&parent_entry.szExeFile[..len]);
                        break;
                    }
                    if Process32NextW(snapshot, &mut parent_entry) == 0 {
                        break;
                    }
                }
            }
        }

        CloseHandle(snapshot);
        parent_name.eq_ignore_ascii_case("explorer.exe")
    }
}

fn item_display_name(item: &ItemPayload) -> String {
    item.name.clone().unwrap_or_else(|| item.id.clone())
}

fn stage_file_name(item: &RelayItemPayload) -> String {
    let base = item
        .name
        .as_deref()
        .map(sanitize_path_fragment)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| item.id.clone());
    format!("{}.part", base)
}

fn sanitize_path_fragment(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
