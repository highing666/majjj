<script lang="ts">
  import QRCode from "qrcode";
  import {
    resolveActiveRemoteEntry,
    type RemoteAccessState,
  } from "./remoteAccessController";

  interface Props {
    state: RemoteAccessState;
    embedded?: boolean;
    onRefresh: () => void;
    onEnable: () => void;
    onDisable: () => void;
    onResetCode: () => void;
    onSavePublicUrl: (publicUrl: string | null) => Promise<boolean>;
    onSelectBaseUrl: (baseUrl: string | null) => void;
  }

  let {
    state: remoteState,
    embedded = false,
    onRefresh,
    onEnable,
    onDisable,
    onResetCode,
    onSavePublicUrl,
    onSelectBaseUrl,
  }: Props = $props();

  let qrDataUrl = $state<string | null>(null);
  let qrError = $state<string | null>(null);
  let copiedKey = $state<string | null>(null);
  let publicUrlDraft = $state("");
  let publicUrlDirty = $state(false);

  const status = $derived(remoteState.status);
  const enabled = $derived(status?.enabled === true);
  const passcode = $derived(status?.passcode ?? null);
  const activeRemoteCount = $derived(status?.active_remote_connections ?? 0);
  const ipv6Urls = $derived(status?.ipv6_urls ?? []);
  const activeEntry = $derived(
    resolveActiveRemoteEntry(status, remoteState.selectedBaseUrl),
  );
  const busy = $derived(remoteState.mutating);

  $effect(() => {
    const persisted = status?.persisted_public_url ?? "";
    if (!publicUrlDirty) {
      publicUrlDraft = persisted;
    }
  });

  $effect(() => {
    const targetUrl = activeEntry.inviteUrl;
    if (!targetUrl) {
      qrDataUrl = null;
      qrError = null;
      return;
    }
    let cancelled = false;
    QRCode.toDataURL(targetUrl, {
      width: 200,
      margin: 2,
      color: {
        dark: "#241b14",
        light: "#fffaf2",
      },
    })
      .then((dataUrl) => {
        if (!cancelled) {
          qrDataUrl = dataUrl;
          qrError = null;
        }
      })
      .catch(() => {
        if (!cancelled) {
          qrDataUrl = null;
          qrError = "邀请二维码生成失败，请直接复制下方链接。";
        }
      });

    return () => {
      cancelled = true;
    };
  });

  async function copyText(text: string, key: string): Promise<void> {
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard) {
        await navigator.clipboard.writeText(text);
        copiedKey = key;
        setTimeout(() => {
          if (copiedKey === key) {
            copiedKey = null;
          }
        }, 1800);
      }
    } catch {
      // Clipboard may be unavailable without user gesture.
    }
  }

  async function handlePublicUrlSubmit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const trimmed = publicUrlDraft.trim();
    const ok = await onSavePublicUrl(trimmed.length > 0 ? trimmed : null);
    if (ok) {
      publicUrlDirty = false;
    }
  }

  async function handleClearPublicUrl(): Promise<void> {
    const ok = await onSavePublicUrl(null);
    if (ok) {
      publicUrlDraft = "";
      publicUrlDirty = false;
    }
  }

  function handleIpv6Select(event: Event): void {
    const raw = (event.currentTarget as HTMLSelectElement).value;
    onSelectBaseUrl(raw === "" ? null : raw);
  }

  function sourceLabel(): string {
    if (activeEntry.usingFallbackIpv6) {
      return "临时切换 · 备用裸 IPv6";
    }
    if (activeEntry.usingCustomDomain) {
      return activeEntry.publicUrlSource === "cli"
        ? "自定义域名 · 启动参数覆盖"
        : "自定义公网域名";
    }
    return "公网 IPv6 地址";
  }
</script>

{#if remoteState.visible}
  <section
    class="remote-panel"
    class:card={!embedded}
    aria-label="宿主远程联机管理"
  >
    <div class="remote-head">
      <div class="remote-title-group">
        {#if !embedded}
          <h2>远程联机</h2>
        {/if}
        <div class="remote-badges">
          <span
            class="tag"
            class:tag-online={enabled}
            class:tag-offline={!enabled}
          >
            {enabled ? "已开启" : "已关闭"}
          </span>
          <span class="tag tag-count" aria-label="当前在线远程连接数">
            在线远程 {activeRemoteCount} 人
          </span>
        </div>
      </div>
      <button
        class="btn btn-subtle refresh-btn"
        type="button"
        disabled={remoteState.loading || busy}
        onclick={onRefresh}
      >
        {remoteState.loading ? "刷新中…" : "刷新"}
      </button>
    </div>

    <p class="remote-subtitle">
      仅宿主机本机回环界面可见。默认关闭；局域网设备始终零口令直连。
    </p>

    <div class="remote-primary-actions">
      {#if !enabled}
        <button
          class="btn btn-jade toggle-btn"
          type="button"
          disabled={busy}
          onclick={onEnable}
        >
          {busy ? "处理中…" : "一键开启远程联机"}
        </button>
      {:else}
        <button
          class="btn btn-danger toggle-btn"
          type="button"
          disabled={busy}
          onclick={onDisable}
        >
          {busy ? "处理中…" : "关闭远程联机"}
        </button>
        <button
          class="btn btn-subtle reset-btn"
          type="button"
          disabled={busy}
          onclick={onResetCode}
        >
          重置口令
        </button>
      {/if}
    </div>

    {#if enabled}
      <p class="revoke-hint">
        「重置口令」仅作废旧口令，不踢已入桌远程牌友；「关闭远程联机」立即断开全部远程连接。
      </p>
    {/if}

    {#if remoteState.errorMessage}
      <div class="remote-feedback remote-feedback-err" role="alert">
        {remoteState.errorMessage}
      </div>
    {:else if remoteState.noticeMessage}
      <div class="remote-feedback remote-feedback-ok" role="status">
        {remoteState.noticeMessage}
      </div>
    {/if}

    {#if enabled && passcode}
      <div class="passcode-card">
        <div class="passcode-head">
          <span class="field-label">当前入桌口令（8 位）</span>
          <button
            class="btn btn-subtle copy-btn"
            type="button"
            onclick={() => copyText(passcode, "code")}
          >
            {copiedKey === "code" ? "已复制" : "复制口令"}
          </button>
        </div>
        <div class="passcode-value" aria-label="当前入桌口令">
          {passcode}
        </div>
      </div>

      {#if activeEntry.inviteUrl}
        <div class="invite-block">
          <div class="invite-head">
            <span class="field-label">当前生效邀请链接</span>
            <span class="tag tag-source">{sourceLabel()}</span>
          </div>

          <div class="qr-frame">
            {#if qrDataUrl}
              <img
                class="qr-image"
                src={qrDataUrl}
                alt="远程联机邀请二维码"
                width="200"
                height="200"
              />
            {:else if qrError}
              <p class="qr-fallback">{qrError}</p>
            {:else}
              <p class="qr-fallback">正在生成邀请二维码…</p>
            {/if}
          </div>

          <div class="url-row">
            <code class="url-code">{activeEntry.inviteUrl}</code>
            <button
              class="btn btn-primary copy-btn"
              type="button"
              onclick={() =>
                activeEntry.inviteUrl &&
                copyText(activeEntry.inviteUrl, "invite")}
            >
              {copiedKey === "invite" ? "已复制" : "复制链接"}
            </button>
          </div>
        </div>
      {:else}
        <div class="empty-entry-hint">
          暂未检测到可用公网 IPv6 地址，也未配置自定义公网域名。可在下方配置自有域名，或检查家庭宽带 IPv6 设置。
        </div>
      {/if}
    {/if}

    <details class="collapsible-section">
      <summary class="collapsible-summary">
        <span>自定义公网域名配置</span>
        {#if status?.public_url_source === "cli"}
          <span class="tag tag-self">启动参数覆盖</span>
        {:else if status?.public_url_source === "preferences"}
          <span class="tag tag-online">已启用</span>
        {:else}
          <span class="tag">可选</span>
        {/if}
      </summary>

      <div class="collapsible-body">
        {#if status?.public_url_source === "cli" && status.public_url_override}
          <div class="cli-override-notice" role="status">
            当前由启动参数（<code>--public-url</code> / <code>MAJJJ_PUBLIC_URL</code>）覆盖生效：
            <strong>{status.public_url_override}</strong>，优先级高于本地配置。
          </div>
        {/if}

        <form class="public-url-form" novalidate onsubmit={handlePublicUrlSubmit}>
          <label class="field-label" for="remote-public-url-input">
            自有公网入口（仅协议 + 域名/IP + 可选端口）
          </label>
          <input
            id="remote-public-url-input"
            class="text-input"
            type="text"
            inputmode="url"
            autocomplete="off"
            spellcheck="false"
            placeholder="http://majjj.example.com:8789"
            value={publicUrlDraft}
            disabled={busy}
            oninput={(e) => {
              publicUrlDraft = (e.currentTarget as HTMLInputElement).value;
              publicUrlDirty = true;
            }}
          />
          <div class="public-url-actions">
            <button class="btn btn-primary small-btn" type="submit" disabled={busy}>
              保存域名
            </button>
            {#if status?.persisted_public_url}
              <button
                class="btn btn-subtle small-btn"
                type="button"
                disabled={busy}
                onclick={handleClearPublicUrl}
              >
                清空本地配置
              </button>
            {/if}
          </div>
        </form>
      </div>
    </details>

    <details class="collapsible-section">
      <summary class="collapsible-summary">
        <span>备选裸 IPv6 地址</span>
        <span class="tag">{ipv6Urls.length} 个</span>
      </summary>

      <div class="collapsible-body">
        {#if ipv6Urls.length > 0}
          <label class="ipv6-picker">
            <span class="field-label">
              {status?.public_url_override
                ? "DDNS 未刷新时可临时改用裸 IPv6 生成邀请链接与二维码"
                : "多网卡或特殊拓扑下可切换公网 IPv6 地址"}
            </span>
            <select
              class="select-input"
              aria-label="备选公网 IPv6 地址"
              value={activeEntry.usingFallbackIpv6 || !status?.public_url_override
                ? (activeEntry.baseUrl ?? "")
                : ""}
              onchange={handleIpv6Select}
            >
              {#if status?.public_url_override}
                <option value="">优先使用自定义域名（{status.public_url_override}）</option>
              {/if}
              {#each ipv6Urls as url, idx (url)}
                <option value={url}>
                  {url}{idx === 0 ? "（首选稳定）" : ""}
                </option>
              {/each}
            </select>
          </label>

          {#if activeEntry.usingFallbackIpv6 && status?.public_url_override}
            <button
              class="btn btn-subtle small-btn"
              type="button"
              onclick={() => onSelectBaseUrl(null)}
            >
              切回自定义公网域名
            </button>
          {/if}
        {:else}
          <p class="empty-ipv6-note">
            暂未检测到公网单播 IPv6 地址（已自动排除 utun/bridge/awdl/llw/lo 虚拟网卡与废弃地址）。
          </p>
        {/if}
      </div>
    </details>
  </section>
{/if}

<style>
  .remote-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .remote-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }

  .remote-title-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    line-height: 1.25;
  }

  .remote-badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .tag-count {
    font-variant-numeric: tabular-nums;
  }

  .refresh-btn {
    padding: 4px 10px;
    font-size: 12px;
    flex: 0 0 auto;
  }

  .remote-subtitle {
    margin: 0;
    font-size: 12px;
    color: var(--ink-muted);
    line-height: 1.4;
  }

  .remote-primary-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .toggle-btn {
    flex: 1 1 150px;
  }

  .reset-btn {
    flex: 0 0 auto;
  }

  .btn-danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .btn-danger:hover:not(:disabled) {
    background: #841b1b;
    border-color: #841b1b;
  }

  .revoke-hint {
    margin: -4px 0 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--ink-muted);
  }

  .remote-feedback {
    padding: 8px 10px;
    border-radius: 10px;
    font-size: 12px;
    line-height: 1.45;
  }

  .remote-feedback-err {
    background: var(--danger-soft);
    border: 1px solid #eab5b5;
    color: var(--danger);
  }

  .remote-feedback-ok {
    background: var(--jade-soft);
    border: 1px solid #9ed4be;
    color: var(--jade);
  }

  .passcode-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid #d4b88c;
    background: #fbf2e3;
  }

  .passcode-head,
  .invite-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }

  .passcode-value {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 24px;
    font-weight: 800;
    letter-spacing: 0.18em;
    text-align: center;
    padding: 6px 8px;
    border-radius: 8px;
    background: #fffaf2;
    border: 1px dashed var(--border);
    color: var(--ink);
    user-select: all;
  }

  .invite-block {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .tag-source {
    font-size: 11px;
  }

  .qr-frame {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 10px;
    border-radius: 12px;
    border: 1px dashed var(--border);
    background: #fffaf2;
  }

  .qr-image {
    display: block;
    max-width: 100%;
    height: auto;
    border-radius: 8px;
  }

  .qr-fallback,
  .empty-entry-hint,
  .empty-ipv6-note {
    margin: 0;
    padding: 10px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-muted);
    border-radius: 10px;
    background: var(--bg-muted);
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .url-code {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--bg-muted);
    border: 1px solid var(--border);
  }

  .copy-btn {
    flex: 0 0 auto;
    padding: 7px 10px;
    font-size: 12px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .collapsible-section {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-muted);
    padding: 8px 10px;
  }

  .collapsible-summary {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    list-style: none;
  }

  .collapsible-summary::-webkit-details-marker {
    display: none;
  }

  .collapsible-body {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .cli-override-notice {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--warn-soft);
    border: 1px solid #efd0a5;
    color: var(--warn);
    font-size: 12px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .public-url-form,
  .ipv6-picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .text-input,
  .select-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: #fff;
    font-size: 13px;
  }

  .public-url-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .small-btn {
    padding: 6px 12px;
    font-size: 12px;
  }
</style>
