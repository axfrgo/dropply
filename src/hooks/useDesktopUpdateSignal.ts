import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useMemo, useState } from "react";

const DEFAULT_UPDATE_URL = "https://dropply.ca/download";
const UPDATE_PREVIEW_EVENT = "dropply:update-preview-changed";
export const UPDATE_PREVIEW_VERSION_KEY = "dropply-update-preview-version";
export const UPDATE_PREVIEW_URL_KEY = "dropply-update-preview-url";

export function announceDesktopUpdatePreviewChange() {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(UPDATE_PREVIEW_EVENT));
  }
}

function readPreviewVersion() {
  if (typeof window === "undefined") {
    return import.meta.env.VITE_DROPPLY_UPDATE_PREVIEW_VERSION ?? null;
  }

  return (
    window.localStorage.getItem(UPDATE_PREVIEW_VERSION_KEY) ??
    import.meta.env.VITE_DROPPLY_UPDATE_PREVIEW_VERSION ??
    null
  );
}

function readUpdateUrl() {
  if (typeof window === "undefined") {
    return import.meta.env.VITE_DROPPLY_UPDATE_URL ?? DEFAULT_UPDATE_URL;
  }

  return (
    window.localStorage.getItem(UPDATE_PREVIEW_URL_KEY) ??
    import.meta.env.VITE_DROPPLY_UPDATE_URL ??
    DEFAULT_UPDATE_URL
  );
}

function toNumericParts(version: string) {
  return version
    .trim()
    .replace(/^v/i, "")
    .split(/[.+-]/)
    .map((part) => Number.parseInt(part, 10))
    .filter((part) => Number.isFinite(part));
}

function compareVersions(left: string, right: string) {
  const leftParts = toNumericParts(left);
  const rightParts = toNumericParts(right);
  const length = Math.max(leftParts.length, rightParts.length);

  for (let index = 0; index < length; index += 1) {
    const leftValue = leftParts[index] ?? 0;
    const rightValue = rightParts[index] ?? 0;

    if (leftValue > rightValue) {
      return 1;
    }

    if (leftValue < rightValue) {
      return -1;
    }
  }

  return 0;
}

export type DesktopUpdateSignal = {
  currentVersion: string | null;
  availableVersion: string | null;
  downloadUrl: string;
  visible: boolean;
};

export function useDesktopUpdateSignal(): DesktopUpdateSignal {
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);
  const [availableVersion, setAvailableVersion] = useState<string | null>(() => readPreviewVersion());
  const [downloadUrl, setDownloadUrl] = useState<string>(() => readUpdateUrl());

  useEffect(() => {
    let mounted = true;

    void getVersion()
      .then((version) => {
        if (mounted) {
          setCurrentVersion(version);
        }
      })
      .catch(() => {
        if (mounted) {
          setCurrentVersion(null);
        }
      });

    const syncPreviewState = () => {
      if (!mounted) {
        return;
      }

      setAvailableVersion(readPreviewVersion());
      setDownloadUrl(readUpdateUrl());
    };

    if (typeof window !== "undefined") {
      window.addEventListener("storage", syncPreviewState);
      window.addEventListener(UPDATE_PREVIEW_EVENT, syncPreviewState);
    }

    return () => {
      mounted = false;
      if (typeof window !== "undefined") {
        window.removeEventListener("storage", syncPreviewState);
        window.removeEventListener(UPDATE_PREVIEW_EVENT, syncPreviewState);
      }
    };
  }, []);

  const visible = useMemo(() => {
    if (!currentVersion || !availableVersion) {
      return false;
    }

    return compareVersions(availableVersion, currentVersion) > 0;
  }, [availableVersion, currentVersion]);

  return {
    currentVersion,
    availableVersion: visible ? availableVersion : null,
    downloadUrl,
    visible,
  };
}
