function resolveWindow() {
  return typeof window === "undefined" ? null : window;
}

export function createFortiStateStore(initialValue) {
  let state = initialValue;
  const subscribers = new Set();

  const notify = () => {
    for (const subscriber of subscribers) {
      subscriber(state);
    }
  };

  return {
    get: () => state,
    set: (value) => {
      state = typeof value === "function" ? value(state) : value;
      notify();
    },
    subscribe: (subscriber) => {
      subscribers.add(subscriber);
      return () => subscribers.delete(subscriber);
    },
    reset: () => {
      state = initialValue;
      notify();
    },
  };
}

export function createPersistentFortiStateStore({
  key,
  fallback,
  read,
  serialize = JSON.stringify,
  deserialize = JSON.parse,
}) {
  const initialValue = (() => {
    const currentWindow = resolveWindow();
    if (!currentWindow) {
      return fallback;
    }

    try {
      if (typeof read === "function") {
        return read();
      }

      const raw = currentWindow.localStorage.getItem(key);
      return raw == null ? fallback : deserialize(raw);
    } catch {
      return fallback;
    }
  })();

  const store = createFortiStateStore(initialValue);
  const originalSet = store.set;
  const originalReset = store.reset;

  store.set = (value) => {
    originalSet(value);
    const currentWindow = resolveWindow();
    if (!currentWindow) {
      return;
    }

    try {
      currentWindow.localStorage.setItem(key, serialize(store.get()));
    } catch {
      // Keep the in-memory state live if persistence fails.
    }
  };

  store.reset = () => {
    originalReset();
    const currentWindow = resolveWindow();
    if (!currentWindow) {
      return;
    }

    try {
      currentWindow.localStorage.removeItem(key);
    } catch {
      // Ignore cleanup failures.
    }
  };

  return store;
}
