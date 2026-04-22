import { useEffect, useState } from "react";
import QRCode from "qrcode";
import type { SyncStatus } from "../lib/types";

type PairingStripProps = {
  syncStatus: SyncStatus;
};

export function PairingStrip({ syncStatus }: PairingStripProps) {
  const [qrDataUrl, setQrDataUrl] = useState<string>("");
  const [copyState, setCopyState] = useState<"idle" | "done">("idle");

  useEffect(() => {
    if (!syncStatus.pairing_token) {
      return;
    }

    const payload = JSON.stringify({
      app: "Dropply",
      pairing_token: syncStatus.pairing_token,
    });

    QRCode.toDataURL(payload, {
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
            <strong>Use the same pairing code on another Dropply device.</strong>
          </div>
          <button type="button" className="ghost" onClick={() => void copyToken()}>
            {copyState === "done" ? "Copied" : "Copy code"}
          </button>
        </div>
        <p className="pairing-copy">
          The QR now contains the raw pairing payload instead of a dead custom URL. If scanning
          is not wired on the other device yet, copy the code directly.
        </p>
        <div className="pairing-token">{syncStatus.pairing_token}</div>
      </div>
      <div className="pairing-qr">
        {qrDataUrl ? <img src={qrDataUrl} alt="QR code containing the Dropply pairing code" /> : null}
      </div>
    </aside>
  );
}
