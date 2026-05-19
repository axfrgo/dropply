import { useSyncExternalStore } from "react";

export function useFortiStateStore(store) {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}
