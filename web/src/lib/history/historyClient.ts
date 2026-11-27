import type { ClearHistoryRequest } from "../protocol/generated/ClearHistoryRequest";
import type { ClearHistoryResponse } from "../protocol/generated/ClearHistoryResponse";
import type { ClearPersonaMemoryRequest } from "../protocol/generated/ClearPersonaMemoryRequest";
import type { ClearPersonaMemoryResponse } from "../protocol/generated/ClearPersonaMemoryResponse";
import type { HistoryExportResponse } from "../protocol/generated/HistoryExportResponse";
import type { PersonaMemoryResponse } from "../protocol/generated/PersonaMemoryResponse";
import type { PreferencesData } from "../protocol/generated/PreferencesData";
import type { StatsResponse } from "../protocol/generated/StatsResponse";

export interface HistoryResponseLike {
  ok: boolean;
  status: number;
  json(): Promise<unknown>;
}

export type HistoryFetch = (
  input: string,
  init?: RequestInit,
) => Promise<HistoryResponseLike>;

export class HistoryClientError extends Error {
  readonly status: number | null;

  constructor(message: string, status: number | null = null) {
    super(message);
    this.name = "HistoryClientError";
    this.status = status;
  }
}

export interface HistoryExportDownload {
  filename: string;
  mimeType: string;
  jsonText: string;
}

export interface HistoryClient {
  fetchStats(): Promise<StatsResponse>;
  fetchPreferences(): Promise<PreferencesData>;
  exportHistory(): Promise<HistoryExportResponse>;
  clearHistory(confirm?: boolean): Promise<ClearHistoryResponse>;
  fetchPersonaMemory(): Promise<PersonaMemoryResponse>;
  clearPersonaMemory(confirm?: boolean): Promise<ClearPersonaMemoryResponse>;
}

export type HistoryExportPhase = "idle" | "exporting" | "success" | "error";
export type HistoryClearPhase = "idle" | "confirming" | "submitting" | "error";

export interface HistoryState {
  open: boolean;
  loading: boolean;
  stats: StatsResponse | null;
  errorMessage: string | null;
  exportPhase: HistoryExportPhase;
  exportNotice: string | null;
  exportErrorMessage: string | null;
  clearPhase: HistoryClearPhase;
  clearNotice: string | null;
  clearErrorMessage: string | null;
  personaMemory: PersonaMemoryResponse | null;
  personaMemoryLoading: boolean;
  personaMemoryErrorMessage: string | null;
  clearMemoryPhase: HistoryClearPhase;
  clearMemoryNotice: string | null;
  clearMemoryErrorMessage: string | null;
}

export type HistoryListener = (state: HistoryState) => void;

export interface HistoryControllerOptions {
  fetchFn?: HistoryFetch;
  onTriggerDownload?: (download: HistoryExportDownload) => void;
}

export interface HistoryController {
  getState(): HistoryState;
  subscribe(listener: HistoryListener): () => void;
  syncClientIsRemote(isRemote: boolean): void;
  open(): Promise<void>;
  close(): void;
  refresh(): Promise<void>;
  requestExport(): Promise<boolean>;
  requestClear(): void;
  cancelClear(): void;
  confirmClear(): Promise<boolean>;
  refreshPersonaMemory(): Promise<void>;
  requestClearMemory(): void;
  cancelClearMemory(): void;
  confirmClearMemory(): Promise<boolean>;
}

function defaultFetch(
  input: string,
  init?: RequestInit,
): Promise<HistoryResponseLike> {
  return fetch(input, init);
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/**
 * Formats a deterministic UTC backup filename (`majjj-history-YYYYMMDD-HHMMSS.json`).
 */
export function formatHistoryExportFilename(exportedAtMs: number): string {
  const date = new Date(exportedAtMs);
  if (Number.isNaN(date.getTime())) {
    return "majjj-history-backup.json";
  }
  const yyyy = date.getUTCFullYear();
  const mm = pad2(date.getUTCMonth() + 1);
  const dd = pad2(date.getUTCDate());
  const hh = pad2(date.getUTCHours());
  const mi = pad2(date.getUTCMinutes());
  const ss = pad2(date.getUTCSeconds());
  return `majjj-history-${yyyy}${mm}${dd}-${hh}${mi}${ss}.json`;
}

/**
 * Serializes `HistoryExportResponse` into a formatted JSON download descriptor.
 */
export function buildHistoryExportDownload(
  data: HistoryExportResponse,
): HistoryExportDownload {
  return {
    filename: formatHistoryExportFilename(data.exported_at_ms),
    mimeType: "application/json;charset=utf-8",
    jsonText: `${JSON.stringify(data, null, 2)}\n`,
  };
}

/**
 * Triggers a browser file download for the serialized history JSON backup.
 */
export function triggerBrowserJsonDownload(
  download: HistoryExportDownload,
): void {
  if (typeof document === "undefined" || typeof URL === "undefined") {
    return;
  }
  const blob = new Blob([download.jsonText], { type: download.mimeType });
  const objectUrl = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = objectUrl;
  link.download = download.filename;
  link.rel = "noopener";
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => {
    URL.revokeObjectURL(objectUrl);
  }, 1000);
}

async function parseErrorMessage(
  res: HistoryResponseLike,
  fallback: string,
): Promise<string> {
  try {
    const body = (await res.json()) as { error?: unknown } | null;
    if (body && typeof body.error === "string" && body.error.trim().length > 0) {
      return `${fallback}（HTTP ${res.status}：${body.error}）`;
    }
  } catch {
    // Ignore invalid JSON body on error responses.
  }
  return `${fallback}（HTTP ${res.status}）`;
}

async function requestJson<T>(
  fetchFn: HistoryFetch,
  url: string,
  init: RequestInit,
  actionLabel: string,
): Promise<T> {
  let response: HistoryResponseLike;
  try {
    response = await fetchFn(url, init);
  } catch {
    throw new HistoryClientError(`无法连接战绩服务，请确认服务端正在运行（${actionLabel}）。`);
  }

  if (!response.ok) {
    const msg = await parseErrorMessage(response, `${actionLabel}失败`);
    throw new HistoryClientError(msg, response.status);
  }

  try {
    return (await response.json()) as T;
  } catch {
    throw new HistoryClientError(
      `${actionLabel}返回数据格式无效`,
      response.status,
    );
  }
}

/**
 * Creates a typed HTTP client for `/api/stats`, `/api/preferences`, `/api/history/export`,
 * and `/api/history/clear`.
 */
export function createHistoryClient(
  fetchFn: HistoryFetch = defaultFetch,
): HistoryClient {
  return {
    fetchStats() {
      return requestJson<StatsResponse>(
        fetchFn,
        "/api/stats",
        { method: "GET", cache: "no-store" },
        "获取战绩统计",
      );
    },
    fetchPreferences() {
      return requestJson<PreferencesData>(
        fetchFn,
        "/api/preferences",
        { method: "GET", cache: "no-store" },
        "获取偏好配置",
      );
    },
    exportHistory() {
      return requestJson<HistoryExportResponse>(
        fetchFn,
        "/api/history/export",
        { method: "GET", cache: "no-store" },
        "导出战绩备份",
      );
    },
    async clearHistory(confirm = true) {
      if (confirm !== true) {
        throw new HistoryClientError("清空战绩必须显式确认（confirm: true）。");
      }
      const payload: ClearHistoryRequest = { confirm: true };
      return requestJson<ClearHistoryResponse>(
        fetchFn,
        "/api/history/clear",
        {
          method: "POST",
          cache: "no-store",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        },
        "清空战绩",
      );
    },
    fetchPersonaMemory() {
      return requestJson<PersonaMemoryResponse>(
        fetchFn,
        "/api/persona-memory",
        { method: "GET", cache: "no-store" },
        "获取牌友记忆",
      );
    },
    async clearPersonaMemory(confirm = true) {
      if (confirm !== true) {
        throw new HistoryClientError(
          "清空牌友记忆必须显式确认（confirm: true）。",
        );
      }
      const payload: ClearPersonaMemoryRequest = { confirm: true };
      return requestJson<ClearPersonaMemoryResponse>(
        fetchFn,
        "/api/persona-memory/clear",
        {
          method: "POST",
          cache: "no-store",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        },
        "清空牌友记忆",
      );
    },
  };
}

/**
 * Pure TypeScript controller for the history panel lifecycle, export flow,
 * persona memory lookup, and explicit two-step clear confirmations with refresh.
 */
export function createHistoryController(
  options: HistoryControllerOptions = {},
): HistoryController {
  const client = createHistoryClient(options.fetchFn ?? defaultFetch);
  const onTriggerDownload =
    options.onTriggerDownload ?? triggerBrowserJsonDownload;
  const listeners = new Set<HistoryListener>();
  let clientIsRemote = false;

  let state: HistoryState = {
    open: false,
    loading: false,
    stats: null,
    errorMessage: null,
    exportPhase: "idle",
    exportNotice: null,
    exportErrorMessage: null,
    clearPhase: "idle",
    clearNotice: null,
    clearErrorMessage: null,
    personaMemory: null,
    personaMemoryLoading: false,
    personaMemoryErrorMessage: null,
    clearMemoryPhase: "idle",
    clearMemoryNotice: null,
    clearMemoryErrorMessage: null,
  };

  function emit(patch: Partial<HistoryState>): void {
    state = { ...state, ...patch };
    for (const listener of listeners) {
      listener(state);
    }
  }

  async function loadStats(): Promise<void> {
    if (clientIsRemote) {
      return;
    }
    emit({ loading: true, errorMessage: null });
    try {
      const stats = await client.fetchStats();
      emit({ loading: false, stats, errorMessage: null });
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : "获取战绩统计失败，请稍后重试。";
      emit({ loading: false, errorMessage: msg });
    }
  }

  async function loadPersonaMemory(): Promise<void> {
    if (clientIsRemote) {
      return;
    }
    emit({ personaMemoryLoading: true, personaMemoryErrorMessage: null });
    try {
      const personaMemory = await client.fetchPersonaMemory();
      emit({
        personaMemoryLoading: false,
        personaMemory,
        personaMemoryErrorMessage: null,
      });
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : "获取牌友记忆失败，请稍后重试。";
      emit({ personaMemoryLoading: false, personaMemoryErrorMessage: msg });
    }
  }

  function createClearConfirmFlow<TRes>(config: {
    phaseKey: "clearPhase" | "clearMemoryPhase";
    noticeKey: "clearNotice" | "clearMemoryNotice";
    errorKey: "clearErrorMessage" | "clearMemoryErrorMessage";
    execute: () => Promise<TRes>;
    formatNotice: (res: TRes) => string;
    fallbackError: string;
    reload: () => Promise<void>;
  }) {
    return {
      request() {
        if (state[config.phaseKey] === "submitting") {
          return;
        }
        emit({
          [config.phaseKey]: "confirming",
          [config.noticeKey]: null,
          [config.errorKey]: null,
        } as Partial<HistoryState>);
      },
      cancel() {
        const phase = state[config.phaseKey];
        if (phase === "confirming" || phase === "error") {
          emit({
            [config.phaseKey]: "idle",
            [config.errorKey]: null,
          } as Partial<HistoryState>);
        }
      },
      async confirm() {
        if (clientIsRemote) {
          return false;
        }
        const phase = state[config.phaseKey];
        if (phase !== "confirming" && phase !== "error") {
          return false;
        }
        emit({
          [config.phaseKey]: "submitting",
          [config.noticeKey]: null,
          [config.errorKey]: null,
        } as Partial<HistoryState>);
        try {
          const res = await config.execute();
          emit({
            [config.phaseKey]: "idle",
            [config.noticeKey]: config.formatNotice(res),
            [config.errorKey]: null,
          } as Partial<HistoryState>);
          await config.reload();
          return true;
        } catch (err) {
          const msg =
            err instanceof Error ? err.message : config.fallbackError;
          emit({
            [config.phaseKey]: "error",
            [config.errorKey]: msg,
          } as Partial<HistoryState>);
          return false;
        }
      },
    };
  }

  const clearHistoryFlow = createClearConfirmFlow({
    phaseKey: "clearPhase",
    noticeKey: "clearNotice",
    errorKey: "clearErrorMessage",
    execute: () => client.clearHistory(true),
    formatNotice: (res) =>
      `已清空 ${res.cleared_matches} 场对局、${res.cleared_hands} 小局历史战绩，默认配置与偏好已保留。`,
    fallbackError: "清空战绩失败，请稍后重试。",
    reload: loadStats,
  });

  const clearMemoryFlow = createClearConfirmFlow({
    phaseKey: "clearMemoryPhase",
    noticeKey: "clearMemoryNotice",
    errorKey: "clearMemoryErrorMessage",
    execute: () => client.clearPersonaMemory(true),
    formatNotice: (res) =>
      `已清空 ${res.cleared_pairs} 组牌友记忆账本与 ${res.cleared_notes} 条恩怨小本本。`,
    fallbackError: "清空牌友记忆失败，请稍后重试。",
    reload: loadPersonaMemory,
  });

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      listener(state);
      return () => {
        listeners.delete(listener);
      };
    },
    syncClientIsRemote(isRemote: boolean) {
      clientIsRemote = isRemote;
      if (isRemote && state.open) {
        emit({
          open: false,
          clearPhase: "idle",
          clearErrorMessage: null,
          clearMemoryPhase: "idle",
          clearMemoryErrorMessage: null,
        });
      }
    },
    async open() {
      if (clientIsRemote) {
        return;
      }
      emit({
        open: true,
        exportPhase: "idle",
        exportNotice: null,
        exportErrorMessage: null,
        clearPhase: "idle",
        clearNotice: null,
        clearErrorMessage: null,
        clearMemoryPhase: "idle",
        clearMemoryNotice: null,
        clearMemoryErrorMessage: null,
      });
      await loadStats();
    },
    close() {
      if (
        state.clearPhase === "submitting" ||
        state.clearMemoryPhase === "submitting"
      ) {
        return;
      }
      emit({
        open: false,
        clearPhase: "idle",
        clearErrorMessage: null,
        clearMemoryPhase: "idle",
        clearMemoryErrorMessage: null,
      });
    },
    async refresh() {
      await loadStats();
    },
    async requestExport() {
      if (clientIsRemote || state.exportPhase === "exporting") {
        return false;
      }
      emit({
        exportPhase: "exporting",
        exportNotice: null,
        exportErrorMessage: null,
      });
      try {
        const exported = await client.exportHistory();
        const download = buildHistoryExportDownload(exported);
        onTriggerDownload(download);
        emit({
          exportPhase: "success",
          exportNotice: `已导出战绩备份文件：${download.filename}`,
          exportErrorMessage: null,
        });
        return true;
      } catch (err) {
        const msg =
          err instanceof Error ? err.message : "导出战绩备份失败，请稍后重试。";
        emit({
          exportPhase: "error",
          exportNotice: null,
          exportErrorMessage: msg,
        });
        return false;
      }
    },
    requestClear: () => clearHistoryFlow.request(),
    cancelClear: () => clearHistoryFlow.cancel(),
    confirmClear: () => clearHistoryFlow.confirm(),
    async refreshPersonaMemory() {
      await loadPersonaMemory();
    },
    requestClearMemory: () => clearMemoryFlow.request(),
    cancelClearMemory: () => clearMemoryFlow.cancel(),
    confirmClearMemory: () => clearMemoryFlow.confirm(),
  };
}
