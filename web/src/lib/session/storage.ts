export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface PersistedIdentity {
  client_id: string;
  nickname: string;
  table_id: string | null;
  seat_token: string | null;
}

export interface SessionStorage {
  loadIdentity(): PersistedIdentity;
  saveNickname(nickname: string): void;
  saveSeatCredential(table_id: string, seat_token: string): void;
  clearSeatCredential(): void;
}

const CLIENT_ID_KEY = "majjj.client_id";
const NICKNAME_KEY = "majjj.nickname";
const SEAT_CREDENTIAL_KEY = "majjj.seat_credential";

/**
 * Generates a UUID v4 string without requiring a secure context (`http://` on LAN Safari).
 */
export function generateUuid(): string {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi && typeof cryptoApi.randomUUID === "function") {
    return cryptoApi.randomUUID();
  }
  if (cryptoApi && typeof cryptoApi.getRandomValues === "function") {
    const bytes = new Uint8Array(16);
    cryptoApi.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0"));
    return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex.slice(6, 8).join("")}-${hex.slice(8, 10).join("")}-${hex.slice(10, 16).join("")}`;
  }
  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

/**
 * Creates a storage adapter persisting `client_id`, `nickname`, and `(table_id, seat_token)`.
 */
export function createSessionStorage(
  storage: StorageLike,
  idGenerator: () => string = generateUuid,
): SessionStorage {
  function readCredential(): { table_id: string; seat_token: string } | null {
    const raw = storage.getItem(SEAT_CREDENTIAL_KEY);
    if (!raw) {
      return null;
    }
    try {
      const parsed: unknown = JSON.parse(raw);
      if (
        typeof parsed === "object" &&
        parsed !== null &&
        "table_id" in parsed &&
        "seat_token" in parsed &&
        typeof parsed.table_id === "string" &&
        typeof parsed.seat_token === "string" &&
        parsed.table_id.length > 0 &&
        parsed.seat_token.length > 0
      ) {
        return { table_id: parsed.table_id, seat_token: parsed.seat_token };
      }
    } catch {
      // Ignore malformed stored credential JSON.
    }
    return null;
  }

  return {
    loadIdentity(): PersistedIdentity {
      let clientId = storage.getItem(CLIENT_ID_KEY)?.trim() ?? "";
      if (clientId.length === 0) {
        clientId = idGenerator();
        storage.setItem(CLIENT_ID_KEY, clientId);
      }
      const nickname = storage.getItem(NICKNAME_KEY) ?? "";
      const credential = readCredential();
      return {
        client_id: clientId,
        nickname,
        table_id: credential?.table_id ?? null,
        seat_token: credential?.seat_token ?? null,
      };
    },

    saveNickname(nickname: string): void {
      storage.setItem(NICKNAME_KEY, nickname.trim());
    },

    saveSeatCredential(table_id: string, seat_token: string): void {
      storage.setItem(
        SEAT_CREDENTIAL_KEY,
        JSON.stringify({ table_id, seat_token }),
      );
    },

    clearSeatCredential(): void {
      storage.removeItem(SEAT_CREDENTIAL_KEY);
    },
  };
}

/**
 * Fallback in-memory storage when `localStorage` is unavailable.
 */
export function createMemoryStorage(): StorageLike {
  const map = new Map<string, string>();
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, value) => {
      map.set(key, value);
    },
    removeItem: (key) => {
      map.delete(key);
    },
  };
}
