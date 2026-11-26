<script lang="ts">
  import type { PrototypeVariantKey } from "./mockState";
  import TraditionalTile from "./TraditionalTile.svelte";
  import artA1Pin from "./generated/art-a-1pin.png";
  import artA1Sou from "./generated/art-a-1sou.png";
  import artB1Pin from "./generated/art-b-1pin.png";
  import artB1Sou from "./generated/art-b-1sou.png";
  import artC1Pin from "./generated/art-c-1pin.png";
  import artC1Sou from "./generated/art-c-1sou.png";

  interface Props {
    open: boolean;
    activeVariant: PrototypeVariantKey;
    onSelectVariant: (variant: PrototypeVariantKey) => void;
    onClose: () => void;
  }

  let { open, activeVariant, onSelectVariant, onClose }: Props = $props();

  const SUIT_GROUPS = [
    { name: "万子（一万～九万）", tiles: [0, 1, 2, 3, 4, 5, 6, 7, 8] },
    { name: "条子（一条～九条）", tiles: [9, 10, 11, 12, 13, 14, 15, 16, 17] },
    { name: "筒子（一筒～九筒）", tiles: [18, 19, 20, 21, 22, 23, 24, 25, 26] },
    { name: "字牌（风牌与箭牌）", tiles: [27, 28, 29, 30, 31, 32, 33] },
  ];

  const STYLE_CARDS: {
    key: PrototypeVariantKey;
    title: string;
    subtitle: string;
    sou1: string;
    pin1: string;
  }[] = [
    {
      key: "A",
      title: "风格 A · 玉骨工笔",
      subtitle: "细腻工笔勾线 + 温润象牙白 + 雅致楷体",
      sou1: artA1Sou,
      pin1: artA1Pin,
    },
    {
      key: "B",
      title: "风格 B · 厚涂国漫",
      subtitle: "浓墨国漫笔触 + 翡翠厚涂质感（贴合 #63 角色）",
      sou1: artB1Sou,
      pin1: artB1Pin,
    },
    {
      key: "C",
      title: "风格 C · 金石碑刻",
      subtitle: "高对比木刻轮廓 + 粗重宋体刻字（小屏最锐利）",
      sou1: artC1Sou,
      pin1: artC1Pin,
    },
  ];
</script>

{#if open}
  <div
    class="gallery-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="34张传统牌面与风格对比"
  >
    <div class="gallery-sheet">
      <header class="gallery-head">
        <div>
          <h2>34 张中国传统麻将牌面 · 混合资产与全站缩略牌对比</h2>
          <p>
            一条/一筒由 AI 风格参考生成，其余万/条/筒/字牌采用确定性传统构图与粗楷刻字；点击任意风格卡可同步切换牌桌样式。
          </p>
        </div>
        <button class="close-btn" type="button" onclick={onClose}>关闭 ✕</button>
      </header>

      <div class="gallery-body">
        <section class="gallery-section">
          <h3>1. AI 风格对比样张（一条幺鸡 &amp; 一筒大花轮）</h3>
          <div class="style-grid">
            {#each STYLE_CARDS as card (card.key)}
              <button
                type="button"
                class="style-card"
                class:style-card-active={activeVariant === card.key}
                onclick={() => onSelectVariant(card.key)}
              >
                <div class="style-card-head">
                  <strong>{card.title}</strong>
                  {#if activeVariant === card.key}
                    <span class="active-chip">当前预览</span>
                  {/if}
                </div>
                <span class="style-sub">{card.subtitle}</span>
                <div class="style-samples">
                  <div class="sample-pair">
                    <span class="sample-tag">原始 AI 样张</span>
                    <div class="raw-pair">
                      <img src={card.sou1} alt="{card.title} 一条原图" />
                      <img src={card.pin1} alt="{card.title} 一筒原图" />
                    </div>
                  </div>
                  <div class="sample-pair">
                    <span class="sample-tag">合成牌面</span>
                    <div class="raw-pair">
                      <TraditionalTile tile={9} variant={card.key} body="standing" width={46} height={63} />
                      <TraditionalTile tile={18} variant={card.key} body="standing" width={46} height={63} />
                      <TraditionalTile tile={4} variant={card.key} body="standing" width={46} height={63} />
                    </div>
                  </div>
                </div>
              </button>
            {/each}
          </div>
        </section>

        <section class="gallery-section split-row">
          <div class="panel-box">
            <h3>2. 立牌体 vs 倒牌体 vs 扣牌体（当前风格 {activeVariant}）</h3>
            <div class="body-compare">
              <div class="body-item">
                <TraditionalTile tile={9} variant={activeVariant} body="standing" width={54} height={74} />
                <span>自家立牌（带翡翠牌背厚度）</span>
              </div>
              <div class="body-item">
                <TraditionalTile tile={9} variant={activeVariant} body="laid" width={54} height={74} />
                <span>牌河 / 鸣牌平铺倒牌体</span>
              </div>
              <div class="body-item">
                <TraditionalTile tile={null} variant={activeVariant} body="back" width={54} height={74} />
                <span>他家暗杠 / 手牌背面</span>
              </div>
              <div class="body-item">
                <TraditionalTile tile={33} variant={activeVariant} body="standing" width={54} height={74} />
                <span>大陆传统纯白光板（无蓝虚线框）</span>
              </div>
            </div>
          </div>

          <div class="panel-box">
            <h3>3. 全站统一 TileBadge 缩略牌预览（动作栏 &amp; 小局结算）</h3>
            <div class="badge-preview">
              <div class="mock-action-strip">
                <span class="mock-act act-hu">
                  <TraditionalTile tile={18} variant={activeVariant} body="badge" width={22} height={30} />
                  <span>胡 一筒</span>
                </span>
                <span class="mock-act act-pon">
                  <TraditionalTile tile={4} variant={activeVariant} body="badge" width={22} height={30} />
                  <span>碰 五万</span>
                </span>
                <span class="mock-act act-kong">
                  <TraditionalTile tile={31} variant={activeVariant} body="badge" width={22} height={30} />
                  <span>明杠 中</span>
                </span>
                <span class="mock-act act-pass">过</span>
              </div>
              <div class="mock-settlement-strip">
                <span class="settle-label">胡牌牌型示例：</span>
                {#each [0, 1, 2, 12, 13, 14, 18, 18, 18, 27, 27, 27, 32, 32] as tile, i (i)}
                  <TraditionalTile
                    {tile}
                    variant={activeVariant}
                    body="badge"
                    width={24}
                    height={33}
                    highlighted={i === 13}
                  />
                {/each}
              </div>
            </div>
          </div>
        </section>

        <section class="gallery-section">
          <h3>4. 全 34 张传统牌面一览（当前风格 {activeVariant}）</h3>
          <div class="all-tiles-groups">
            {#each SUIT_GROUPS as group (group.name)}
              <div class="suit-row">
                <div class="suit-label">{group.name}</div>
                <div class="suit-tiles">
                  {#each group.tiles as t (t)}
                    <TraditionalTile
                      tile={t}
                      variant={activeVariant}
                      body="standing"
                      width={46}
                      height={63}
                    />
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </section>
      </div>
    </div>
  </div>
{/if}

<style>
  .gallery-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9500;
    background: rgba(6, 14, 18, 0.78);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 18px;
  }

  .gallery-sheet {
    width: min(1180px, 100%);
    max-height: 90dvh;
    background: #102026;
    color: #f5eedf;
    border-radius: 18px;
    border: 1.5px solid rgba(212, 168, 87, 0.52);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .gallery-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 20px;
    border-bottom: 1px solid rgba(212, 168, 87, 0.25);
    background: #0c191e;
  }

  .gallery-head h2 {
    margin: 0;
    font-size: 17px;
    color: #f5be58;
  }

  .gallery-head p {
    margin: 4px 0 0;
    font-size: 12.5px;
    color: #cbd5e1;
  }

  .close-btn {
    padding: 6px 12px;
    border-radius: 9px;
    border: 1px solid rgba(212, 168, 87, 0.45);
    background: rgba(255, 255, 255, 0.06);
    color: #f5eedf;
    font-size: 13px;
    font-weight: 700;
    white-space: nowrap;
  }

  .gallery-body {
    padding: 16px 20px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .gallery-section h3 {
    margin: 0 0 10px;
    font-size: 14px;
    color: #f5eedf;
  }

  .style-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 12px;
  }

  .style-card {
    text-align: left;
    padding: 12px;
    border-radius: 13px;
    border: 1.5px solid rgba(212, 168, 87, 0.28);
    background: rgba(255, 255, 255, 0.04);
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .style-card-active {
    border-color: #f5be58;
    background: rgba(245, 190, 88, 0.12);
    box-shadow: 0 0 0 1px rgba(245, 190, 88, 0.4);
  }

  .style-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 14px;
  }

  .active-chip {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 999px;
    background: #f5be58;
    color: #181a20;
  }

  .style-sub {
    font-size: 12px;
    color: #cbd5e1;
  }

  .style-samples {
    margin-top: 6px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }

  .sample-pair {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .sample-tag {
    font-size: 11px;
    color: #94a3b8;
  }

  .raw-pair {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .raw-pair img {
    width: 48px;
    height: 64px;
    object-fit: cover;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: #f6efe0;
  }

  .split-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 12px;
  }

  .panel-box {
    padding: 12px 14px;
    border-radius: 13px;
    border: 1px solid rgba(212, 168, 87, 0.26);
    background: rgba(255, 255, 255, 0.03);
  }

  .body-compare {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .body-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: #cbd5e1;
    text-align: center;
    max-width: 110px;
  }

  .badge-preview {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .mock-action-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 11px;
    background: #0b161b;
    border: 1px solid rgba(212, 168, 87, 0.4);
  }

  .mock-act {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 8px;
    font-size: 12.5px;
    font-weight: 800;
    border: 1.5px solid #181a20;
    color: #fff;
  }

  .act-hu {
    background: #b92822;
    border-color: #f5be58;
  }
  .act-pon {
    background: #b97517;
  }
  .act-kong {
    background: #176963;
  }
  .act-pass {
    background: #33464e;
  }

  .mock-settlement-strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 8px 10px;
    border-radius: 11px;
    background: rgba(255, 255, 255, 0.05);
  }

  .settle-label {
    font-size: 12px;
    color: #cbd5e1;
    margin-right: 4px;
  }

  .all-tiles-groups {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .suit-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.035);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .suit-label {
    width: 145px;
    font-size: 12.5px;
    font-weight: 700;
    color: #e2e8f0;
  }

  .suit-tiles {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
</style>
