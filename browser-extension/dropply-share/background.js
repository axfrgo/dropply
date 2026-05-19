const BRIDGE_BASE_URL = "http://127.0.0.1:45123/v1/browser-share";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "dropply-share-page",
    title: "Send page to Dropply",
    contexts: ["page", "action"],
  });

  chrome.contextMenus.create({
    id: "dropply-share-selection",
    title: "Send selection to Dropply",
    contexts: ["selection"],
  });
});

chrome.action.onClicked.addListener((tab) => {
  void shareTab(tab, "page");
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (!tab) {
    return;
  }

  if (info.menuItemId === "dropply-share-selection") {
    void shareTab(tab, "selection");
    return;
  }

  if (info.menuItemId === "dropply-share-page") {
    void shareTab(tab, "page");
  }
});

async function shareTab(tab, mode) {
  if (!tab.id || !tab.url || !/^https?:/i.test(tab.url)) {
    await notify("Dropply Share", "Open a normal web page first, then try again.");
    return;
  }

  try {
    await ensureBridgeIsReady();
  } catch (error) {
    await notify(
      "Dropply Share",
      error instanceof Error ? error.message : "Open the Dropply desktop app first."
    );
    return;
  }

  let payload;
  try {
    payload = await chrome.tabs.sendMessage(tab.id, {
      type: "dropply-collect-bundle",
      mode,
    });
  } catch (error) {
    await notify(
      "Dropply Share",
      error instanceof Error
        ? error.message
        : "Dropply could not read this page. Reload it and try again."
    );
    return;
  }

  if (!payload || !payload.transcript_markdown) {
    await notify("Dropply Share", "Dropply could not extract a transcript from this page.");
    return;
  }

  try {
    const response = await fetch(`${BRIDGE_BASE_URL}/bundle`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });

    const result = await response.json().catch(() => null);
    if (!response.ok || !result?.ok) {
      throw new Error(result?.error || "Dropply rejected the browser bundle.");
    }

    await notify(
      "Sent to Dropply",
      `${payload.title || "Smart Drop bundle"} was added to your Dropply stream.`
    );
  } catch (error) {
    await notify(
      "Dropply Share",
      error instanceof Error ? error.message : "Dropply could not receive this bundle."
    );
  }
}

async function ensureBridgeIsReady() {
  let response;
  try {
    response = await fetch(`${BRIDGE_BASE_URL}/health`, { method: "GET" });
  } catch {
    throw new Error("Open the Dropply desktop app first so the local browser bridge is running.");
  }

  if (!response.ok) {
    throw new Error("Dropply's local browser bridge is not ready yet.");
  }

  const result = await response.json().catch(() => null);
  if (!result?.ok) {
    throw new Error(result?.error || "Dropply's local browser bridge is not ready yet.");
  }
}

function notify(title, message) {
  return chrome.notifications.create({
    type: "basic",
    iconUrl: chrome.runtime.getURL("icon.png"),
    title,
    message,
  });
}
