# Dropply Simple Guide

Last updated: 2026-05-14

This is the easy version of how Dropply works today.

## 1. What Dropply is

Dropply lets your Windows desktop share Smart Drops with your phone, browser, or terminal:

- text
- photos
- videos
- files

Your desktop is the main device. The web page is the helper screen that joins the desktop.

A Smart Drop is the thing plus useful local context:

- the file, text, image, link, or browser bundle
- where it came from
- what device or page it came from when Dropply knows
- a local label or short preview
- suggested next actions
- lifecycle state such as captured, pending, completed, or revoked

Smart Drops v1 does this with local rules. It does not use cloud AI.

Language support now:

- desktop app: English and French
- pair page: English and French

## 2. What the two network modes mean

### `p2p`

This means:

- Dropply tries to send media directly between devices
- best when both devices are online and reachable
- if the direct link is not there, media should not pretend it can still load from relay in this mode

Good for:

- fast direct transfers
- local-device style behavior

### `relay`

This means:

- Dropply uploads the media to the hosted backend in chunks
- the other device downloads it from the backend
- it does not need the direct media link to stay alive
- relay chunk bytes can now live in FortiBuckets instead of only sitting in backend memory

Good for:

- browser and phone reliability
- cases where direct transfer is awkward or unavailable

## 3. What happens when you pair

1. The desktop shows a token or QR.
2. The phone or browser opens the pair page.
3. That second device joins the same token.
4. Both sides now see the same stream of items.

Device behavior now adapts better:

- phones are treated as phones
- desktop browsers are treated as browsers

So if you manually open the pair page on a desktop browser, it should no longer pretend that browser is a phone.

## 4. What you can do today

From the desktop:

- send text
- drop in files
- see Smart Drop labels, source, tags, actions, and state
- open files in the default desktop app
- download items straight to Downloads
- mark items pending, completed, or revoked
- remove one paired device
- unpair the desktop

From the phone or browser:

- view incoming items
- see Smart Drop labels, source, tags, actions, and state
- preview images and videos
- download files
- send text or files back to the desktop

From the CLI/TUI:

- pair into the same Dropply session
- send text or files from a terminal
- search and preview Smart Drops
- open, mark pending, mark completed, or revoke Smart Drop intent

## 5. Download behavior

### Desktop app

- downloads go straight into the Windows Downloads folder

### Browser or phone

- downloads go wherever the browser normally saves downloads

If a browser is set to ask every time, the browser still gets to ask.

## 6. Simple limits by thing type

| Thing | Best path | Important limit to know |
| --- | --- | --- |
| Text | stream metadata | text does not have its own chunked blob path today, so huge text is a bad fit |
| Photo | direct or relay | relay uses `128 KiB` chunks when needed |
| Video | direct or relay | relay uses `128 KiB` chunks when needed |
| File | direct or relay | relay uses `128 KiB` chunks when needed |
| Smart Drop metadata | stream metadata | labels, source context, suggested actions, and lifecycle state stay small and sync with the item |

## 7. The most important real limits

- direct transfer chunk size: `64 KiB`
- relay blob chunk size: `128 KiB`
- relay manifest total budget: about `480 KiB`
- inline bytes inside that relay manifest: about `192 KiB`
- backend request body limit: `50 MiB` per request

What that means in normal language:

- small item metadata is cheap
- big media should be chunked
- giant inline base64 payloads are a bad idea

## 8. Plan limits vs transport limits

There are two different kinds of limits.

### Transport limits

These are the technical limits Dropply uses while moving data:

- chunk sizes
- manifest budgets
- request body caps

### Plan limits

These are the product numbers published by the backend:

- Free: `500` synced items, `25 MB` upload, `3` devices
- Pro: `10,000` synced items, `250 MB` upload, `12` devices

These are not the same thing.

## 9. Why text, photo, and video now work better

The current app fixes several earlier problems:

- relay media is uploaded in chunks before metadata is published
- relay item downloads rebuild valid base64 again
- the desktop fetches full relay item bytes before importing media
- the transport toggle now actually changes how media is supposed to move

## 10. If something looks broken

### CORS error on relay push

Often really means:

- the request was too large
- the proxy rejected it before normal headers were attached

### "Missing file bytes"

Usually means:

- the client got metadata without the real media bytes

### direct media link error after a successful transfer

Often means:

- the browser or phone closed the connection after the transfer already finished

## 11. The short version

If you want the shortest possible explanation:

- desktop is the boss
- every item can become a Smart Drop
- `p2p` means direct device-to-device media
- `relay` means hosted chunked media
- relay blobs are now stored more durably through FortiBuckets
- downloads now behave sensibly
- phone and browser behavior are now separated properly
- desktop, web dashboard, and TUI now speak the same Smart Drop metadata shape
