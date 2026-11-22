import { describe, expect, it } from "vitest";
import { createSessionStorage, type StorageLike } from "./storage";

class MemoryStorage implements StorageLike {
  private readonly map = new Map<string, string>();

  getItem(key: string): string | null {
    return this.map.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.map.set(key, value);
  }

  removeItem(key: string): void {
    this.map.delete(key);
  }
}

describe("session storage", () => {
  it("generates and persists client_id when storage is empty, then reuses it", () => {
    const raw = new MemoryStorage();
    let counter = 0;
    const store = createSessionStorage(raw, () => `client-${++counter}`);

    const first = store.loadIdentity();
    expect(first).toEqual({
      client_id: "client-1",
      nickname: "",
      table_id: null,
      seat_token: null,
    });

    const second = store.loadIdentity();
    expect(second.client_id).toBe("client-1");
    expect(counter).toBe(1);
  });

  it("persists nickname and table seat credentials across reloads and clears corrupt credentials safely", () => {
    const raw = new MemoryStorage();
    const store = createSessionStorage(raw, () => "generated-id");

    store.loadIdentity();
    store.saveNickname("小明");
    store.saveSeatCredential("table-42", "token-abc");

    expect(store.loadIdentity()).toEqual({
      client_id: "generated-id",
      nickname: "小明",
      table_id: "table-42",
      seat_token: "token-abc",
    });

    raw.setItem("majjj.seat_credential", "{broken-json");
    expect(store.loadIdentity()).toEqual({
      client_id: "generated-id",
      nickname: "小明",
      table_id: null,
      seat_token: null,
    });

    store.saveSeatCredential("table-43", "token-xyz");
    store.clearSeatCredential();
    expect(store.loadIdentity().table_id).toBeNull();
    expect(store.loadIdentity().seat_token).toBeNull();
  });
});
