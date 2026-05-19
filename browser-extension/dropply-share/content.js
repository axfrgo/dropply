const MAX_TRANSCRIPT_CHARS = 750000;
const MAX_CODE_ATTACHMENTS = 12;
const MAX_TURNS = 80;

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== "dropply-collect-bundle") {
    return false;
  }

  Promise.resolve()
    .then(() => collectBundlePayload(message.mode || "page"))
    .then(sendResponse)
    .catch((error) => {
      sendResponse({
        title: buildBundleTitle(detectSiteLabel()),
        source_label: detectSiteLabel(),
        source_url: window.location.href,
        transcript_markdown: `# Browser share failed\n\nDropply could not extract this page:\n\n${String(
          error?.message || error
        )}`,
        files: [],
        attachments: [],
      });
    });

  return true;
});

function collectBundlePayload(mode) {
  const siteLabel = detectSiteLabel();
  const selectionText = normalizeText(window.getSelection?.()?.toString() || "");
  const extraction = extractTranscript(mode, selectionText);
  const attachments = buildAttachments(extraction, selectionText);

  return {
    title: buildBundleTitle(siteLabel),
    source_label: siteLabel,
    source_url: window.location.href,
    transcript_markdown: extraction.transcriptMarkdown,
    files: [],
    attachments,
  };
}

function extractTranscript(mode, selectionText) {
  if (mode === "selection" && selectionText) {
    const transcriptMarkdown = clampText(
      `# ${document.title || "Selection"}\n\n${selectionText}`
    );
    return {
      transcriptMarkdown,
      codeBlocks: collectCodeBlocks(document),
      turnCount: 1,
      strategy: "selection",
    };
  }

  const turnSets = [
    detectChatGptTurns(),
    detectClaudeTurns(),
    detectGenericArticleTurns(),
    detectMessageLikeTurns(),
  ];
  const turns = turnSets.find((items) => items.length > 0) || [];

  if (turns.length > 0) {
    const transcriptMarkdown = clampText(
      turns
        .slice(0, MAX_TURNS)
        .map((turn, index) => {
          const heading = `## ${turn.role || `Turn ${index + 1}`}`;
          const body = normalizeText(turn.element.innerText || "");
          return `${heading}\n\n${body}`;
        })
        .filter(Boolean)
        .join("\n\n")
    );

    return {
      transcriptMarkdown: `# ${document.title || "Conversation"}\n\n${transcriptMarkdown}`,
      codeBlocks: collectCodeBlocksFromTurns(turns),
      turnCount: turns.length,
      strategy: "conversation-turns",
    };
  }

  const mainText = normalizeText(
    document.querySelector("main")?.innerText || document.body?.innerText || ""
  );
  const transcriptMarkdown = clampText(
    `# ${document.title || "Shared page"}\n\n${mainText || selectionText || "No visible text was available."}`
  );

  return {
    transcriptMarkdown,
    codeBlocks: collectCodeBlocks(document),
    turnCount: 1,
    strategy: "page-fallback",
  };
}

function buildAttachments(extraction, selectionText) {
  const attachments = [];
  const metadata = {
    title: document.title || "Untitled page",
    source_url: window.location.href,
    extracted_at: new Date().toISOString(),
    strategy: extraction.strategy,
    turn_count: extraction.turnCount,
    selected_text_present: Boolean(selectionText),
    code_block_count: extraction.codeBlocks.length,
    user_agent: navigator.userAgent,
  };

  attachments.push({
    name: "metadata.json",
    archive_path: "attachments/metadata.json",
    mime_type: "application/json",
    text_content: JSON.stringify(metadata, null, 2),
  });

  if (selectionText) {
    attachments.push({
      name: "selection.md",
      archive_path: "attachments/selection.md",
      mime_type: "text/markdown",
      text_content: clampText(selectionText),
    });
  }

  extraction.codeBlocks.slice(0, MAX_CODE_ATTACHMENTS).forEach((block, index) => {
    const extension = block.language ? sanitizeExtension(block.language) : "txt";
    const name = `code-block-${String(index + 1).padStart(2, "0")}.${extension}`;
    attachments.push({
      name,
      archive_path: `attachments/code/${name}`,
      mime_type: "text/plain",
      text_content: clampText(block.code),
    });
  });

  return attachments;
}

function detectChatGptTurns() {
  return Array.from(document.querySelectorAll("[data-message-author-role]"))
    .filter(isVisibleAndTextual)
    .map((element) => ({
      role: formatRole(element.getAttribute("data-message-author-role")),
      element,
    }));
}

function detectClaudeTurns() {
  const selectors = [
    ["main [data-testid='user-message']", "User"],
    ["main [data-testid='assistant-message']", "Assistant"],
    ["main [data-testid='conversation-turn']", null],
  ];
  const turns = [];

  selectors.forEach(([selector, explicitRole]) => {
    document.querySelectorAll(selector).forEach((element) => {
      if (isVisibleAndTextual(element)) {
        turns.push({
          role: explicitRole || inferRole(element),
          element,
        });
      }
    });
  });

  return dedupeTurns(turns);
}

function detectGenericArticleTurns() {
  return dedupeTurns(
    Array.from(document.querySelectorAll("main article"))
      .filter(isVisibleAndTextual)
      .map((element, index) => ({
        role: inferRole(element) || `Turn ${index + 1}`,
        element,
      }))
  );
}

function detectMessageLikeTurns() {
  const selectors = [
    "main [class*='message']",
    "main [class*='Message']",
    "main [data-testid*='message']",
  ];
  return dedupeTurns(
    selectors
      .flatMap((selector) => Array.from(document.querySelectorAll(selector)))
      .filter(isVisibleAndTextual)
      .map((element, index) => ({
        role: inferRole(element) || `Turn ${index + 1}`,
        element,
      }))
  );
}

function collectCodeBlocksFromTurns(turns) {
  return collectCodeBlocks({
    querySelectorAll(selector) {
      return turns.flatMap((turn) => Array.from(turn.element.querySelectorAll(selector)));
    },
  });
}

function collectCodeBlocks(root) {
  const seen = new Set();
  const blocks = [];
  const nodes = Array.from(root.querySelectorAll("pre code, pre"));

  nodes.forEach((node) => {
    const code = normalizeText(node.innerText || "");
    if (!code || seen.has(code)) {
      return;
    }
    seen.add(code);
    blocks.push({
      language: detectCodeLanguage(node),
      code,
    });
  });

  return blocks;
}

function detectCodeLanguage(node) {
  const candidates = [node, node.closest("pre"), node.parentElement].filter(Boolean);
  for (const candidate of candidates) {
    const match = candidate.className?.match(/language-([a-z0-9+#-]+)/i);
    if (match?.[1]) {
      return match[1].toLowerCase();
    }
    const attr =
      candidate.getAttribute?.("data-language") ||
      candidate.getAttribute?.("data-lang") ||
      candidate.getAttribute?.("lang");
    if (attr) {
      return String(attr).toLowerCase();
    }
  }
  return null;
}

function inferRole(element) {
  const label = [
    element.getAttribute("data-message-author-role"),
    element.getAttribute("aria-label"),
    element.getAttribute("data-testid"),
    element.className,
    element.innerText.split("\n")[0],
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();

  if (label.includes("user") || label.includes("you")) {
    return "User";
  }
  if (label.includes("assistant") || label.includes("chatgpt") || label.includes("claude")) {
    return "Assistant";
  }
  return null;
}

function dedupeTurns(turns) {
  const seen = new Set();
  const deduped = [];
  turns.forEach((turn) => {
    const key = normalizeText(turn.element.innerText || "");
    if (!key || seen.has(key)) {
      return;
    }
    seen.add(key);
    deduped.push(turn);
  });
  return deduped;
}

function isVisibleAndTextual(element) {
  if (!element || !(element instanceof Element)) {
    return false;
  }
  if (!element.innerText || normalizeText(element.innerText).length < 16) {
    return false;
  }
  const rect = element.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function detectSiteLabel() {
  const host = window.location.hostname.toLowerCase();
  if (host.includes("chatgpt.com") || host.includes("openai.com")) {
    return "ChatGPT";
  }
  if (host.includes("perplexity.ai")) {
    return "Perplexity";
  }
  if (host.includes("claude.ai")) {
    return "Claude";
  }
  return document.title || host;
}

function buildBundleTitle(siteLabel) {
  const title = document.title?.trim() || "Conversation";
  return `${siteLabel} - ${title}`.slice(0, 140);
}

function normalizeText(value) {
  return String(value || "")
    .replace(/\r\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\u00a0/g, " ")
    .trim();
}

function clampText(value) {
  const text = normalizeText(value);
  if (text.length <= MAX_TRANSCRIPT_CHARS) {
    return text;
  }
  return `${text.slice(0, MAX_TRANSCRIPT_CHARS)}\n\n[Truncated by Dropply Share]`;
}

function formatRole(role) {
  if (!role) {
    return "Turn";
  }
  const normalized = String(role).trim().toLowerCase();
  if (normalized === "user") {
    return "User";
  }
  if (normalized === "assistant" || normalized === "tool") {
    return "Assistant";
  }
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}

function sanitizeExtension(language) {
  return String(language || "txt")
    .toLowerCase()
    .replace(/[^a-z0-9+-]/g, "")
    .slice(0, 12) || "txt";
}
