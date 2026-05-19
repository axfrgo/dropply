# Dropply Share Extension

This is the unpacked browser extension for sending the current conversation or page into Dropply as a sandboxed Smart Drop bundle.

## What it does

- Sends the current page or selected text to the local Dropply desktop app
- Packages the transcript as the bundle body
- Adds `metadata.json` plus extracted code blocks as inline bundle attachments
- Preserves page title and URL as Smart Drop source context
- Lets the desktop classify the bundle locally with no cloud AI call
- Uses Dropply's local sandboxed browser bridge on `http://127.0.0.1:45123`

## Load it in Chrome or Edge

1. Open `chrome://extensions` or `edge://extensions`
2. Turn on `Developer mode`
3. Click `Load unpacked`
4. Select this folder:

`C:\Users\alexj\Documents\OpenDrop\browser-extension\dropply-share`

## Use it

1. Open the Dropply desktop app so the local bridge is running
2. Open ChatGPT, Perplexity, Claude, or any normal web page
3. Click the `Dropply Share` extension button

You can also right-click:

- `Send page to Dropply`
- `Send selection to Dropply`

## Notes

- The browser bridge only accepts extension origins, not normal web pages
- This MVP extracts visible conversation/page text from the DOM; it does not use private site APIs
- Browser shares already go through the same Dropply sandbox and conversation-bundle validation path as desktop and CLI bundles
- In v1.0.0, browser shares appear as Smart Drops with source, label, tags, suggested actions, and lifecycle state in desktop/TUI
