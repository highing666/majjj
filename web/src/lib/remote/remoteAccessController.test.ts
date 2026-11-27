import { describe, expect, it, vi } from "vitest";
import type { RemoteAccessStatusResponse } from "../protocol/generated/RemoteAccessStatusResponse";
import {
  buildRemoteToolbarLabel,
  composeRemoteInviteUrl,
  createRemoteAccessController,
  resolveActiveRemoteEntry,
  type RemoteAccessFetch,
} from "./remoteAccessController";

function makeStatus(
  overrides: Partial<RemoteAccessStatusResponse> = {},
): RemoteAccessStatusResponse {
  return {
    enabled: false,
    passcode: null,
    active_remote_connections: 0,
    public_url_override: null,
    public_url_source: "none",
    persisted_public_url: null,
    preferred_ipv6_url: "http://[2001:db9:1::10]:8789",
    ipv6_urls: [
      "http://[2001:db9:1::10]:8789",
      "http://[2001:db9:1::20]:8789",
    ],
    invite_base_url: "http://[2001:db9:1::10]:8789",
    invite_url: null,
    ...overrides,
  };
}

function mockJsonResponse(
  status: number,
  body: unknown,
): { ok: boolean; status: number; json(): Promise<unknown> } {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  };
}

describe("remoteAccessController", () => {
  it("strictly refuses all HTTP requests when host_is_loopback is false", async () => {
    const fetchFn = vi.fn<RemoteAccessFetch>();
    const ctrl = createRemoteAccessController({ fetchFn });

    expect(ctrl.getState().visible).toBe(false);

    ctrl.syncHostIsLoopback(false);
    expect(await ctrl.refresh()).toBeNull();
    expect(await ctrl.enable()).toBe(false);
    expect(await ctrl.disable()).toBe(false);
    expect(await ctrl.resetCode()).toBe(false);
    expect(await ctrl.savePublicUrl("http://majjj.example.com:8789")).toBe(
      false,
    );

    expect(fetchFn).not.toHaveBeenCalled();
  });

  it("auto-fetches status on loopback activation and handles enable, resetCode, savePublicUrl, and disable", async () => {
    const initial = makeStatus();
    const enabled = makeStatus({
      enabled: true,
      passcode: "ABCD2345",
      active_remote_connections: 1,
      invite_url: "http://[2001:db9:1::10]:8789/?code=ABCD2345",
    });
    const rotated = makeStatus({
      enabled: true,
      passcode: "WXYZ6789",
      active_remote_connections: 1,
      invite_url: "http://[2001:db9:1::10]:8789/?code=WXYZ6789",
    });
    const withCustomDomain = makeStatus({
      enabled: true,
      passcode: "WXYZ6789",
      active_remote_connections: 2,
      public_url_override: "http://majjj.example.com:8789",
      public_url_source: "preferences",
      persisted_public_url: "http://majjj.example.com:8789",
      invite_base_url: "http://majjj.example.com:8789",
      invite_url: "http://majjj.example.com:8789/?code=WXYZ6789",
    });
    const disabled = makeStatus({
      enabled: false,
      passcode: null,
      active_remote_connections: 0,
      public_url_override: "http://majjj.example.com:8789",
      public_url_source: "preferences",
      persisted_public_url: "http://majjj.example.com:8789",
      invite_base_url: "http://majjj.example.com:8789",
      invite_url: null,
    });

    const fetchFn = vi.fn<RemoteAccessFetch>(async (url, init) => {
      if (url === "/api/remote-access" && (!init || init.method === "GET")) {
        return mockJsonResponse(200, initial);
      }
      if (url === "/api/remote-access/enable") {
        expect(init?.method).toBe("POST");
        expect(init?.headers).toEqual({ "Content-Type": "application/json" });
        return mockJsonResponse(200, enabled);
      }
      if (url === "/api/remote-access/reset-code") {
        expect(init?.method).toBe("POST");
        expect(init?.headers).toEqual({ "Content-Type": "application/json" });
        return mockJsonResponse(200, rotated);
      }
      if (url === "/api/remote-access/public-url") {
        expect(init?.method).toBe("POST");
        expect(init?.headers).toEqual({ "Content-Type": "application/json" });
        expect(init?.body).toBe(
          JSON.stringify({ public_url: "http://majjj.example.com:8789" }),
        );
        return mockJsonResponse(200, withCustomDomain);
      }
      if (url === "/api/remote-access/disable") {
        expect(init?.method).toBe("POST");
        expect(init?.headers).toEqual({ "Content-Type": "application/json" });
        return mockJsonResponse(200, disabled);
      }
      throw new Error(`unexpected fetch: ${url}`);
    });

    const ctrl = createRemoteAccessController({ fetchFn });
    ctrl.syncHostIsLoopback(true);
    await vi.waitFor(() => {
      expect(ctrl.getState().status?.enabled).toBe(false);
    });
    expect(ctrl.getState().visible).toBe(true);

    expect(await ctrl.enable()).toBe(true);
    expect(ctrl.getState().status?.passcode).toBe("ABCD2345");
    expect(ctrl.getState().noticeMessage).toContain("已开启");

    expect(await ctrl.resetCode()).toBe(true);
    expect(ctrl.getState().status?.passcode).toBe("WXYZ6789");
    expect(ctrl.getState().noticeMessage).toContain("不受影响");

    // Select fallback IPv6, then save a custom domain -> selection resets to newly saved domain.
    ctrl.selectBaseUrl("http://[2001:db9:1::20]:8789");
    expect(ctrl.getState().selectedBaseUrl).toBe(
      "http://[2001:db9:1::20]:8789",
    );

    expect(await ctrl.savePublicUrl("http://majjj.example.com:8789")).toBe(
      true,
    );
    expect(ctrl.getState().selectedBaseUrl).toBeNull();
    expect(ctrl.getState().status?.public_url_source).toBe("preferences");

    expect(await ctrl.disable()).toBe(true);
    expect(ctrl.getState().status?.enabled).toBe(false);
    expect(ctrl.getState().status?.passcode).toBeNull();

    // Dropping loopback hides panel and clears state without making further requests.
    const callsBeforeDrop = fetchFn.mock.calls.length;
    ctrl.syncHostIsLoopback(false);
    expect(ctrl.getState().visible).toBe(false);
    expect(ctrl.getState().status).toBeNull();
    expect(fetchFn.mock.calls.length).toBe(callsBeforeDrop);
  });

  it("surfaces server validation error messages and CLI override notices clearly", async () => {
    const cliStatus = makeStatus({
      enabled: true,
      passcode: "ABCD2345",
      public_url_override: "https://cli.example.com:8443",
      public_url_source: "cli",
      persisted_public_url: "http://pref.example.com:8789",
      invite_base_url: "https://cli.example.com:8443",
      invite_url: "https://cli.example.com:8443/?code=ABCD2345",
    });

    const fetchFn = vi.fn<RemoteAccessFetch>(async (url, init) => {
      if (url === "/api/remote-access") {
        return mockJsonResponse(200, cliStatus);
      }
      if (url === "/api/remote-access/public-url") {
        const parsed = JSON.parse(String(init?.body ?? "{}")) as {
          public_url: string | null;
        };
        if (parsed.public_url?.includes("/bad")) {
          return mockJsonResponse(400, {
            error: "自定义公网入口不能包含路径",
          });
        }
        return mockJsonResponse(200, cliStatus);
      }
      return mockJsonResponse(500, { error: "unexpected" });
    });

    const ctrl = createRemoteAccessController({ fetchFn });
    ctrl.syncHostIsLoopback(true);
    await vi.waitFor(() => {
      expect(ctrl.getState().status?.public_url_source).toBe("cli");
    });

    expect(
      await ctrl.savePublicUrl("http://majjj.example.com:8789/bad"),
    ).toBe(false);
    expect(ctrl.getState().errorMessage).toBe("自定义公网入口不能包含路径");

    expect(await ctrl.savePublicUrl("http://pref.example.com:8789")).toBe(true);
    expect(ctrl.getState().errorMessage).toBeNull();
    expect(ctrl.getState().noticeMessage).toContain("启动参数");
  });

  it("resolves active invite URL across custom domain and collapsible raw IPv6 fallback", () => {
    expect(composeRemoteInviteUrl("http://majjj.example.com:8789/", "ABCD2345")).toBe(
      "http://majjj.example.com:8789/?code=ABCD2345",
    );
    expect(composeRemoteInviteUrl(null, "ABCD2345")).toBeNull();

    const status = makeStatus({
      enabled: true,
      passcode: "ABCD2345",
      public_url_override: "http://majjj.example.com:8789",
      public_url_source: "preferences",
      persisted_public_url: "http://majjj.example.com:8789",
      invite_base_url: "http://majjj.example.com:8789",
      invite_url: "http://majjj.example.com:8789/?code=ABCD2345",
    });

    const primary = resolveActiveRemoteEntry(status, null);
    expect(primary.baseUrl).toBe("http://majjj.example.com:8789");
    expect(primary.inviteUrl).toBe("http://majjj.example.com:8789/?code=ABCD2345");
    expect(primary.usingCustomDomain).toBe(true);
    expect(primary.usingFallbackIpv6).toBe(false);

    const fallback = resolveActiveRemoteEntry(
      status,
      "http://[2001:db9:1::20]:8789",
    );
    expect(fallback.baseUrl).toBe("http://[2001:db9:1::20]:8789");
    expect(fallback.inviteUrl).toBe("http://[2001:db9:1::20]:8789/?code=ABCD2345");
    expect(fallback.usingCustomDomain).toBe(false);
    expect(fallback.usingFallbackIpv6).toBe(true);

    // Stale selection no longer in ipv6_urls falls back safely to invite_base_url.
    const stale = resolveActiveRemoteEntry(
      status,
      "http://[2001:db9:9::99]:8789",
    );
    expect(stale.baseUrl).toBe("http://majjj.example.com:8789");
    expect(stale.usingCustomDomain).toBe(true);
  });

  it("formats compact in-match toolbar label with live remote count badge", () => {
    expect(buildRemoteToolbarLabel(null)).toBe("远程联机");
    expect(buildRemoteToolbarLabel(makeStatus({ enabled: false }))).toBe(
      "远程联机",
    );
    expect(
      buildRemoteToolbarLabel(
        makeStatus({ enabled: true, active_remote_connections: 0 }),
      ),
    ).toBe("远程 · 0");
    expect(
      buildRemoteToolbarLabel(
        makeStatus({ enabled: true, active_remote_connections: 2 }),
      ),
    ).toBe("远程 · 2");
  });
});
