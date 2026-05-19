export type FortiStateSubscriber<T> = (value: T) => void;

export type FortiStateStore<T> = {
  get: () => T;
  set: (value: T | ((current: T) => T)) => void;
  subscribe: (subscriber: FortiStateSubscriber<T>) => () => void;
  reset: () => void;
};

export type PersistentStoreOptions<T> = {
  key: string;
  fallback: T;
  read?: () => T;
  serialize?: (value: T) => string;
  deserialize?: (raw: string) => T;
};

export declare function createFortiStateStore<T>(initialValue: T): FortiStateStore<T>;
export declare function createPersistentFortiStateStore<T>(
  options: PersistentStoreOptions<T>
): FortiStateStore<T>;
