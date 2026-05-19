import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { useI18n } from "../lib/i18n";
import type { SyncStatus } from "../lib/types";
import type { PairDevice } from "../lib/relay";
import type { RelayTransferVisual } from "../hooks/useDropply";

const DEFAULT_PAIR_PORTAL_BASE_URL = "https://dropply.ca";
const pairPortalBaseUrl = (
  import.meta.env.VITE_WEB_BASE_URL ?? DEFAULT_PAIR_PORTAL_BASE_URL
).replace(/\/$/, "");

type PairingStripProps = {
  syncStatus: SyncStatus;
  pairedDevices: PairDevice[];
  relayUploadProgress: RelayTransferVisual | null;
  onSetToken: (token: string) => Promise<void>;
  networkMode: "relay" | "p2p";
  onToggleNetwork: (mode: "relay" | "p2p") => void;
  onResetToken: () => void;
  onUnpair: () => void;
  onRemoveDevice: (device: PairDevice) => Promise<void>;
};

export function PairingStrip({
  syncStatus,
  pairedDevices,
  relayUploadProgress,
  onSetToken,
  networkMode,
  onToggleNetwork,
  onResetToken,
  onUnpair,
  onRemoveDevice,
}: PairingStripProps) {
  const { formatDeviceType, formatRelativeTime, t } = useI18n();
  const [qrDataUrl, setQrDataUrl] = useState<string>("");
  const [copyState, setCopyState] = useState<"idle" | "done">("idle");
  const [isJoining, setIsJoining] = useState(false);
  const [joinToken, setJoinToken] = useState("");
  const hasQrCode = qrDataUrl.length > 0;
  const syncLive = syncStatus.transport !== "offline" || syncStatus.paired_devices > 0;

  useEffect(() => {
    if (!syncStatus.pairing_token) {
      return;
    }

    const pairUrl = `${pairPortalBaseUrl}/pair?token=${encodeURIComponent(syncStatus.pairing_token)}`;

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

  async function handleJoinSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!joinToken.trim()) return;
    await onSetToken(joinToken.trim());
    setIsJoining(false);
    setJoinToken("");
  }

  function formatTransferBytes(bytes: number) {
    if (bytes >= 1024 * 1024 * 1024) {
      return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
    }

    if (bytes >= 1024 * 1024) {
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    if (bytes >= 1024) {
      return `${(bytes / 1024).toFixed(1)} KB`;
    }

    return `${bytes} B`;
  }

  return (
    <aside className="pairing-strip" aria-label="Device pairing">
      <div className="pairing-heading">
        <div className="pairing-heading-top">
          <div>
            <span className="badge">{t("pairDevices")}</span>
            <h2 className="section-title">
              {syncStatus.paired_devices > 0 ? t("connectedDevices") : t("scanToConnect")}
            </h2>
          </div>
          <div className="pairing-heading-status">
            <div className="network-toggle">
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  checked={networkMode === "relay"}
                  onChange={(e) => onToggleNetwork(e.target.checked ? "relay" : "p2p")}
                />
                <span className="toggle-slider"></span>
                <span className="toggle-label">
                  {networkMode === "relay" ? t("relayMedia") : t("directMedia")}
                </span>
              </label>
            </div>
          </div>
        </div>
        <div className="pairing-actions">
          <button type="button" className="ghost" title={t("reset")} onClick={() => onResetToken()}>
            {t("reset")}
          </button>
          <button
            type="button"
            className="ghost destructive"
            title={t("unpairDesktop")}
            onClick={() => onUnpair()}
          >
            {t("unpairDesktop")}
          </button>
          <button type="button" className="ghost" onClick={() => setIsJoining(!isJoining)}>
            {isJoining ? t("cancel") : t("joinSession")}
          </button>
          <button type="button" className="ghost" onClick={() => void copyToken()}>
            {copyState === "done" ? t("copied") : t("copyCode")}
          </button>
        </div>
      </div>
      <div className="pairing-body">
        <div className="pairing-content">
        <p className="pairing-copy">
          {syncLive
            ? networkMode === "relay"
              ? t("pairCopyRelay")
              : t("pairCopyDirect")
            : t("pairCopyIdle")}
        </p>

        {isJoining ? (
          <form onSubmit={handleJoinSubmit} style={{ display: "flex", gap: "0.5rem", marginTop: "0.5rem" }}>
            <input
              type="text"
              placeholder={t("pastePairingToken")}
              value={joinToken}
              onChange={(e) => setJoinToken(e.target.value)}
              style={{
                flex: 1,
                padding: "0.4rem 0.6rem",
                borderRadius: "0.4rem",
                border: "1px solid var(--border)",
              }}
              autoFocus
            />
            <button
              type="submit"
              className="solid-button"
              style={{ padding: "0.4rem 0.8rem", fontSize: "0.9rem" }}
            >
              {t("join")}
            </button>
          </form>
        ) : (
          <div className="pairing-token">{syncStatus.pairing_token}</div>
        )}

        {relayUploadProgress ? (
          <div className="transfer-progress">
            <div className="transfer-progress__copy">
              <strong>{relayUploadProgress.label}</strong>
              <span>{relayUploadProgress.message}</span>
            </div>
            <div className="transfer-progress__track" aria-hidden="true">
              <span style={{ width: `${relayUploadProgress.percent}%` }} />
            </div>
            <div className="transfer-progress__meta">
              <span>
                {relayUploadProgress.sentChunks}/{relayUploadProgress.totalChunks} chunks
              </span>
              <span>
                {formatTransferBytes(relayUploadProgress.sentBytes)} /{" "}
                {formatTransferBytes(relayUploadProgress.totalBytes)}
              </span>
            </div>
          </div>
        ) : null}

        {pairedDevices.length > 0 ? (
          <div className="pairing-device-list" aria-label="Paired devices">
            {pairedDevices.map((device) => {
              const isCurrentDevice = device.deviceId === syncStatus.device_id;

              return (
                <div key={device.deviceId} className="pairing-device-row">
                  <div className="pairing-device-meta">
                    <strong>
                      {isCurrentDevice ? t("thisDesktop", { label: device.label }) : device.label}
                    </strong>
                    <span>
                      {formatDeviceType(device.deviceType)} · {formatRelativeTime(device.lastSeenAt)}
                    </span>
                  </div>
                  {isCurrentDevice ? (
                    <span className="pairing-device-chip">{t("current")}</span>
                  ) : (
                    <button
                      type="button"
                      className="ghost destructive"
                      onClick={() => void onRemoveDevice(device)}
                    >
                      {t("remove")}
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        ) : null}
      </div>
        {hasQrCode ? (
          <div className="pairing-qr">
            <img src={qrDataUrl} alt={t("qrAlt")} />
          </div>
        ) : null}
      </div>
    </aside>
  );
}
