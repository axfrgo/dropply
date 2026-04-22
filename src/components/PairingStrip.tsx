import { useEffect, useState } from "react";
import QRCode from "qrcode";
import type { SyncStatus } from "../lib/types";

type PairingStripProps = {
  syncStatus: SyncStatus;
};

export function PairingStrip({ syncStatus }: PairingStripProps) {
  const [qrDataUrl, setQrDataUrl] = useState<string>("");
  const [copyState, setCopyState] = useState<"idle" | "done">("idle");
  const syncLive = syncStatus.transport !== "offline" || syncStatus.paired_devices > 0;

  useEffect(() => {
    if (!syncStatus.pairing_token) {
      return;
    }

    const pairUrl = `https://getdropply.vercel.app/pair?token=${encodeURIComponent(syncStatus.pairing_token)}`;

    QRCode.toDataURL(pairUrl, {
      width: 108,
      margin: 1,
      color: {
        dark: "#1b2433",
        light: "#fbfdff",
      },
    })
      .then(setQrDataUrl)
      .catch(() => setQrDataUrl(""));
  }, [syncStatus.pairing_token]);

  async function copyToken() {
    await navigator.clipboard.writeText(syncStatus.pairing_token);
    setCopyState("done");
    window.setTimeout(() => setCopyState("idle"), 1400);
  }

  return (
    <aside className="pairing-strip" aria-label="Device pairing">
      <div className="pairing-content">
        <div className="pairing-heading">
          <div>
            <p className="eyebrow">Pair devices</p>
            <strong>
              {syncLive
                ? "Use the same pairing code on another Dropply device."
                : "Cross-device sync is not live in this build yet."}
            </strong>
          </div>
          <button type="button" className="ghost" onClick={() => void copyToken()}>
            {copyState === "done" ? "Copied" : "Copy code"}
          </button>
        </div>
        <p className="pairing-copy">
          {syncLive
            ? "Scan the code or copy the token to connect another device to the same live stream."
            : "The QR currently only carries a pairing token. Real desktop-to-phone transfer is not implemented in this release, so scanning will not sync files yet."}
        </p>
        <div className="pairing-token">{syncStatus.pairing_token}</div>
      </div>
      <div className="pairing-qr">
        {qrDataUrl ? <img src={qrDataUrl} alt="QR code containing the Dropply pairing code" /> : null}
      </div>
    </aside>
  );
}
