import {
  createFortiStateStore,
  createPersistentFortiStateStore,
} from "@dropply/fortistate-kernel";
import { useFortiStateStore } from "@dropply/fortistate-kernel/react";

export type Locale = "en" | "fr";
export type NetworkMode = "relay" | "p2p";

const LOCALE_STORAGE_KEY = "dropply-locale";
const NETWORK_MODE_STORAGE_KEY = "dropply-network-mode";

function detectLocale(): Locale {
  if (typeof window === "undefined") {
    return "en";
  }

  const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
  if (stored === "en" || stored === "fr") {
    return stored;
  }

  const languageList = navigator.languages?.join(",") || navigator.language || "en";
  return languageList.toLowerCase().includes("fr") ? "fr" : "en";
}

function detectNetworkMode(): NetworkMode {
  if (typeof window === "undefined") {
    return "relay";
  }

  return window.localStorage.getItem(NETWORK_MODE_STORAGE_KEY) === "p2p" ? "p2p" : "relay";
}

export const localeStore = createPersistentFortiStateStore<Locale>({
  key: LOCALE_STORAGE_KEY,
  fallback: "en",
  read: detectLocale,
});

export const networkModeStore = createPersistentFortiStateStore<NetworkMode>({
  key: NETWORK_MODE_STORAGE_KEY,
  fallback: "relay",
  read: detectNetworkMode,
});

export const cloudAuthAvailableStore = createFortiStateStore(false);

export function useLocaleStore() {
  return useFortiStateStore(localeStore);
}

export function useNetworkModeStore() {
  return useFortiStateStore(networkModeStore);
}

export function useCloudAuthAvailableStore() {
  return useFortiStateStore(cloudAuthAvailableStore);
}
