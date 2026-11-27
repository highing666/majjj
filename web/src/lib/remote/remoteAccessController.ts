import type { PublicUrlSource } from "../protocol/generated/PublicUrlSource";
import type { RemoteAccessStatusResponse } from "../protocol/generated/RemoteAccessStatusResponse";
import type { SetPublicUrlRequest } from "../protocol/generated/SetPublicUrlRequest";

export interface RemoteAccessResponseLike {
  ok: boolean;
  status: number;
  json(): Promise<unknown>;
}

export interface RemoteAccessRequestInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

export type RemoteAccessFetch = (
  input: string,
  init?: RemoteAccessRequestInit,
) => Promise<RemoteAccessResponseLike>;

export interface ActiveRemoteEntry {
  baseUrl: string | null;
  inviteUrl: string | null;
  usingCustomDomain: boolean;
  usingFallbackIpv6: boolean;
  publicUrlSource: PublicUrlSource;
}

export interface RemoteAccessState {
  visible: boolean;
  status: RemoteAccessStatusResponse | null;
  loading: boolean;
  mutating: boolean;
  selectedBaseUrl: string | null;
  errorMessage: string | null;
  noticeMessage: string | null;
}

export interface RemoteAccessControllerOptions {
  fetchFn?: RemoteAccessFetch;
}

export type RemoteAccessListener = (state: RemoteAccessState) => void;

export interface RemoteAccessController {
  getState(): RemoteAccessState;
  subscribe(listener: RemoteAccessListener): () => void;
  syncHostIsLoopback(hostIsLoopback: boolean): void;
  refresh(): Promise<RemoteAccessStatusResponse | null>;
  enable(): Promise<boolean>;
  disable(): Promise<boolean>;
  resetCode(): Promise<boolean>;
  savePublicUrl(publicUrl: string | null): Promise<boolean>;
  selectBaseUrl(baseUrl: string | null): void;
  clearFeedback(): void;
}

const JSON_HEADERS: Record<string, string> = {
  "Content-Type": "application/json",
};

function defaultFetch(
  input: string,
  init?: RemoteAccessRequestInit,
): Promise<RemoteAccessResponseLike> {
  return fetch(input, init);
}

/**
 * Composes a shareable invite link (`<origin>/?code=<passcode>`) from a base origin and passcode.
 */
export function composeRemoteInviteUrl(
  baseUrl: string | null,
  passcode: string | null,
): string | null {
  if (!baseUrl || !passcode) {
    return null;
  }
  const trimmedBase = baseUrl.trim().replace(/\/+$/, "");
  const trimmedCode = passcode.trim();
  if (!trimmedBase || !trimmedCode) {
    return null;
  }
  return `${trimmedBase}/?code=${encodeURIComponent(trimmedCode)}`;
}

/**
 * Resolves the currently selected shareable base URL and composed invite URL,
 * supporting temporary switching between the custom public domain and any discovered raw IPv6 URL.
 */
export function resolveActiveRemoteEntry(
  status: RemoteAccessStatusResponse | null,
  selectedBaseUrl: string | null,
): ActiveRemoteEntry {
  if (!status) {
    return {
      baseUrl: null,
      inviteUrl: null,
      usingCustomDomain: false,
      usingFallbackIpv6: false,
      publicUrlSource: "none",
    };
  }

  const hasCustomDomain = status.public_url_override !== null;
  let baseUrl = status.invite_base_url;
  let usingFallbackIpv6 = false;

  if (selectedBaseUrl !== null) {
    if (status.ipv6_urls.includes(selectedBaseUrl)) {
      baseUrl = selectedBaseUrl;
      usingFallbackIpv6 = hasCustomDomain;
    } else if (selectedBaseUrl === status.public_url_override) {
      baseUrl = selectedBaseUrl;
      usingFallbackIpv6 = false;
    }
  }

  const usingCustomDomain =
    hasCustomDomain && baseUrl === status.public_url_override;
  const inviteUrl = status.enabled
    ? composeRemoteInviteUrl(baseUrl, status.passcode)
    : null;

  return {
    baseUrl,
    inviteUrl,
    usingCustomDomain,
    usingFallbackIpv6,
    publicUrlSource: status.public_url_source,
  };
}

/**
 * Formats the compact in-match top toolbar button label for the host remote-access trigger.
 */
export function buildRemoteToolbarLabel(
  status: RemoteAccessStatusResponse | null,
): string {
  if (!status?.enabled) {
    return "远程联机";
  }
  return `远程 · ${status.active_remote_connections}`;
}

async function extractServerError(
  res: RemoteAccessResponseLike,
  fallback: string,
): Promise<string> {
  try {
    const data = await res.json();
    if (
      data !== null &&
      typeof data === "object" &&
      "error" in data &&
      typeof (data as { error: unknown }).error === "string" &&
      (data as { error: string }).error.trim().length > 0
    ) {
      return (data as { error: string }).error;
    }
  } catch {
    // Response may be an empty 403 body.
  }
  if (res.status === 403) {
    return "远程联机控制被拒绝（HTTP 403）：仅允许宿主机本机回环界面操作。";
  }
  return `${fallback}（HTTP ${res.status}）`;
}

/**
 * Pure DOM-free state controller for Loopback-only `/api/remote-access*` endpoints.
 * Strictly refuses to issue any HTTP request when `host_is_loopback` is `false`.
 */
export function createRemoteAccessController(
  options: RemoteAccessControllerOptions = {},
): RemoteAccessController {
  const fetchFn = options.fetchFn ?? defaultFetch;
  const listeners = new Set<RemoteAccessListener>();

  let hostIsLoopback = false;
  let status: RemoteAccessStatusResponse | null = null;
  let loading = false;
  let mutating = false;
  let selectedBaseUrl: string | null = null;
  let errorMessage: string | null = null;
  let noticeMessage: string | null = null;
  let syncEpoch = 0;

  function snapshotState(): RemoteAccessState {
    return {
      visible: hostIsLoopback,
      status,
      loading,
      mutating,
      selectedBaseUrl,
      errorMessage,
      noticeMessage,
    };
  }

  function emit(): void {
    const next = snapshotState();
    for (const listener of listeners) {
      listener(next);
    }
  }

  function reconcileSelectedBaseUrl(nextStatus: RemoteAccessStatusResponse): void {
    if (selectedBaseUrl === null) {
      return;
    }
    const stillValid =
      selectedBaseUrl === nextStatus.public_url_override ||
      nextStatus.ipv6_urls.includes(selectedBaseUrl);
    if (!stillValid) {
      selectedBaseUrl = null;
    }
  }

  async function refresh(): Promise<RemoteAccessStatusResponse | null> {
    if (!hostIsLoopback) {
      return null;
    }
    const epoch = syncEpoch;
    loading = true;
    emit();

    try {
      const res = await fetchFn("/api/remote-access", { method: "GET" });
      if (epoch !== syncEpoch || !hostIsLoopback) {
        return null;
      }
      if (!res.ok) {
        errorMessage = await extractServerError(res, "获取远程联机状态失败");
        loading = false;
        emit();
        return null;
      }
      const nextStatus = (await res.json()) as RemoteAccessStatusResponse;
      if (epoch !== syncEpoch || !hostIsLoopback) {
        return null;
      }
      status = nextStatus;
      reconcileSelectedBaseUrl(nextStatus);
      errorMessage = null;
      loading = false;
      emit();
      return nextStatus;
    } catch {
      if (epoch !== syncEpoch || !hostIsLoopback) {
        return null;
      }
      errorMessage = "无法连接本机服务，获取远程联机状态失败。";
      loading = false;
      emit();
      return null;
    }
  }

  async function postControlAction(
    url: string,
    body: unknown,
    fallbackError: string,
    buildNotice: (next: RemoteAccessStatusResponse) => string,
    resetSelection = false,
  ): Promise<boolean> {
    if (!hostIsLoopback || mutating) {
      return false;
    }
    const epoch = syncEpoch;
    mutating = true;
    errorMessage = null;
    emit();

    try {
      const res = await fetchFn(url, {
        method: "POST",
        headers: JSON_HEADERS,
        body: JSON.stringify(body),
      });
      if (epoch !== syncEpoch || !hostIsLoopback) {
        mutating = false;
        return false;
      }
      if (!res.ok) {
        errorMessage = await extractServerError(res, fallbackError);
        noticeMessage = null;
        mutating = false;
        emit();
        return false;
      }
      const nextStatus = (await res.json()) as RemoteAccessStatusResponse;
      if (epoch !== syncEpoch || !hostIsLoopback) {
        mutating = false;
        return false;
      }
      status = nextStatus;
      if (resetSelection) {
        selectedBaseUrl = null;
      } else {
        reconcileSelectedBaseUrl(nextStatus);
      }
      errorMessage = null;
      noticeMessage = buildNotice(nextStatus);
      mutating = false;
      emit();
      return true;
    } catch {
      if (epoch !== syncEpoch || !hostIsLoopback) {
        mutating = false;
        return false;
      }
      errorMessage = "无法连接本机服务，请稍后重试。";
      noticeMessage = null;
      mutating = false;
      emit();
      return false;
    }
  }

  return {
    getState: snapshotState,
    subscribe(listener) {
      listeners.add(listener);
      listener(snapshotState());
      return () => {
        listeners.delete(listener);
      };
    },
    syncHostIsLoopback(nextLoopback) {
      if (hostIsLoopback === nextLoopback) {
        return;
      }
      syncEpoch += 1;
      hostIsLoopback = nextLoopback;
      if (!hostIsLoopback) {
        status = null;
        loading = false;
        mutating = false;
        selectedBaseUrl = null;
        errorMessage = null;
        noticeMessage = null;
        emit();
        return;
      }
      emit();
      void refresh();
    },
    refresh,
    enable() {
      return postControlAction(
        "/api/remote-access/enable",
        {},
        "开启远程联机失败",
        () => "远程联机已开启，已生成新的入桌口令。",
      );
    },
    disable() {
      return postControlAction(
        "/api/remote-access/disable",
        {},
        "关闭远程联机失败",
        () => "远程联机已关闭，已断开全部远程连接并作废远程会话。",
        true,
      );
    },
    resetCode() {
      return postControlAction(
        "/api/remote-access/reset-code",
        {},
        "重置入桌口令失败",
        () => "入桌口令已重置，旧口令立即失效，已入桌远程牌友不受影响。",
      );
    },
    savePublicUrl(publicUrl) {
      const payload: SetPublicUrlRequest = { public_url: publicUrl };
      const clearing = publicUrl === null || publicUrl.trim().length === 0;
      return postControlAction(
        "/api/remote-access/public-url",
        payload,
        "保存自定义公网入口失败",
        (next) => {
          if (next.public_url_source === "cli") {
            return clearing
              ? "已清空本地自定义公网域名；当前仍由启动参数 / 环境变量覆盖生效。"
              : "已保存至本地偏好；当前仍由启动参数 / 环境变量覆盖生效。";
          }
          return clearing
            ? "已清空自定义公网域名，自动回退至公网 IPv6 地址。"
            : "自定义公网域名已保存并立即生效。";
        },
        true,
      );
    },
    selectBaseUrl(baseUrl) {
      if (!hostIsLoopback) {
        return;
      }
      selectedBaseUrl = baseUrl;
      emit();
    },
    clearFeedback() {
      if (errorMessage !== null || noticeMessage !== null) {
        errorMessage = null;
        noticeMessage = null;
        emit();
      }
    },
  };
}
