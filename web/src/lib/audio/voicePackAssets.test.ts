import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ALL_VOICE_PACK_IDS,
  ALL_VOICE_SLOTS,
  VOICE_ACTION_SLOTS,
  VOICE_EMOTE_SLOTS,
  VOICE_TILE_SLOTS,
  parseVoicePackManifest,
} from "./voicePackSchema";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PUBLIC_VO_DIR = path.resolve(__dirname, "../../../public/assets/vo");
const MAX_TOTAL_M4A_BYTES = 2 * 1024 * 1024; // 2.0 MiB single-binary budget ceiling

function findMp4Box(
  buf: Buffer,
  start: number,
  end: number,
  targetType: string,
): { offset: number; end: number } | null {
  let pos = start;
  while (pos + 8 <= end) {
    let size = buf.readUInt32BE(pos);
    const type = buf.toString("ascii", pos + 4, pos + 8);
    let headerSize = 8;
    if (size === 1) {
      if (pos + 16 > end) break;
      size = Number(buf.readBigUInt64BE(pos + 8));
      headerSize = 16;
    } else if (size === 0) {
      size = end - pos;
    }
    if (size < headerSize || pos + size > end) {
      break;
    }
    if (type === targetType) {
      return { offset: pos + headerSize, end: pos + size };
    }
    if (type === "moov") {
      const nested = findMp4Box(buf, pos + headerSize, pos + size, targetType);
      if (nested) return nested;
    }
    pos += size;
  }
  return null;
}

function readM4aDurationSeconds(buf: Buffer): number {
  const mvhd = findMp4Box(buf, 0, buf.length, "mvhd");
  if (!mvhd) {
    throw new Error("missing MP4 mvhd box");
  }
  const version = buf.readUInt8(mvhd.offset);
  let timescale: number;
  let duration: number;
  if (version === 1) {
    timescale = buf.readUInt32BE(mvhd.offset + 20);
    duration = Number(buf.readBigUInt64BE(mvhd.offset + 24));
  } else {
    timescale = buf.readUInt32BE(mvhd.offset + 12);
    duration = buf.readUInt32BE(mvhd.offset + 16);
  }
  if (!timescale || timescale <= 0) {
    throw new Error(`invalid mvhd timescale: ${timescale}`);
  }
  return duration / timescale;
}

describe("VoicePack manifest parser and committed 7-pack assets", () => {
  it("parseVoicePackManifest accepts valid manifests and rejects malformed payloads", () => {
    const valid = parseVoicePackManifest({
      pack_id: "quick_hothead",
      version: 1,
      audio: "sprite.m4a",
      sample_rate: 24000,
      duration: 2.5,
      sprites: {
        m1: [0, 0.5],
        pong: [0.6, 0.4],
      },
    });
    expect(valid).not.toBeNull();
    expect(valid?.pack_id).toBe("quick_hothead");
    expect(valid?.sprites.m1).toEqual([0, 0.5]);

    expect(parseVoicePackManifest(null)).toBeNull();
    expect(parseVoicePackManifest("invalid")).toBeNull();
    expect(parseVoicePackManifest({ pack_id: "", version: 1, audio: "sprite.m4a", sample_rate: 24000, duration: 1, sprites: {} })).toBeNull();
    expect(parseVoicePackManifest({ pack_id: "quick_hothead", version: 0, audio: "sprite.m4a", sample_rate: 24000, duration: 1, sprites: {} })).toBeNull();
    expect(parseVoicePackManifest({ pack_id: "quick_hothead", version: 1, audio: "", sample_rate: 24000, duration: 1, sprites: {} })).toBeNull();
    expect(parseVoicePackManifest({ pack_id: "quick_hothead", version: 1, audio: "sprite.m4a", sample_rate: 24000, duration: -1, sprites: {} })).toBeNull();
    expect(
      parseVoicePackManifest({
        pack_id: "quick_hothead",
        version: 1,
        audio: "sprite.m4a",
        sample_rate: 24000,
        duration: 2,
        sprites: { m1: [-0.1, 0.5] },
      }),
    ).toBeNull();
    expect(
      parseVoicePackManifest({
        pack_id: "quick_hothead",
        version: 1,
        audio: "sprite.m4a",
        sample_rate: 24000,
        duration: 2,
        sprites: { m1: [0, 0] },
      }),
    ).toBeNull();
  });

  it("ships all 7 playable VoicePacks covering 46 canonical slots within the 2.0 MB total budget", () => {
    expect(ALL_VOICE_PACK_IDS).toHaveLength(7);
    expect(VOICE_TILE_SLOTS).toHaveLength(34);
    expect(VOICE_ACTION_SLOTS).toHaveLength(6);
    expect(VOICE_EMOTE_SLOTS).toHaveLength(6);
    expect(ALL_VOICE_SLOTS).toHaveLength(46);

    let totalM4aBytes = 0;

    for (const packId of ALL_VOICE_PACK_IDS) {
      const packDir = path.join(PUBLIC_VO_DIR, packId);
      const manifestPath = path.join(packDir, "manifest.json");
      const spritePath = path.join(packDir, "sprite.m4a");

      expect(fs.existsSync(manifestPath), `missing ${manifestPath}`).toBe(true);
      expect(fs.existsSync(spritePath), `missing ${spritePath}`).toBe(true);

      const rawManifest = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as unknown;
      const manifest = parseVoicePackManifest(rawManifest);
      expect(manifest, `${packId}/manifest.json failed parseVoicePackManifest`).not.toBeNull();
      expect(manifest!.pack_id).toBe(packId);
      expect(manifest!.version).toBe(1);
      expect(manifest!.audio).toBe("sprite.m4a");
      expect(manifest!.sample_rate).toBe(24000);
      expect(manifest!.duration).toBeGreaterThan(0);

      const spriteKeys = Object.keys(manifest!.sprites);
      expect(spriteKeys).toHaveLength(ALL_VOICE_SLOTS.length);
      expect(new Set(spriteKeys)).toEqual(new Set(ALL_VOICE_SLOTS));

      const m4aBuf = fs.readFileSync(spritePath);
      expect(m4aBuf.length, `${packId}/sprite.m4a must be non-empty`).toBeGreaterThan(1024);
      totalM4aBytes += m4aBuf.length;

      const mvhdDuration = readM4aDurationSeconds(m4aBuf);
      expect(mvhdDuration).toBeGreaterThan(0);

      const orderedSlices = ALL_VOICE_SLOTS.map((slotId) => {
        const slice = manifest!.sprites[slotId];
        expect(slice, `${packId} missing slice for ${slotId}`).toBeDefined();
        return { slotId, start: slice![0], duration: slice![1] };
      }).sort((a, b) => a.start - b.start);

      for (let i = 0; i < orderedSlices.length; i += 1) {
        const cur = orderedSlices[i]!;
        expect(cur.start, `${packId}:${cur.slotId} start >= 0`).toBeGreaterThanOrEqual(0);
        expect(cur.duration, `${packId}:${cur.slotId} duration > 0`).toBeGreaterThan(0);
        expect(
          cur.start + cur.duration,
          `${packId}:${cur.slotId} within manifest.duration`,
        ).toBeLessThanOrEqual(manifest!.duration + 0.001);
        expect(
          cur.start + cur.duration,
          `${packId}:${cur.slotId} within mvhd duration`,
        ).toBeLessThanOrEqual(mvhdDuration + 0.05);

        if (i > 0) {
          const prev = orderedSlices[i - 1]!;
          expect(
            cur.start,
            `${packId}: ${cur.slotId} overlaps ${prev.slotId}`,
          ).toBeGreaterThanOrEqual(prev.start + prev.duration - 1e-6);
        }
      }
    }

    expect(totalM4aBytes).toBeGreaterThan(0);
    expect(totalM4aBytes).toBeLessThanOrEqual(MAX_TOTAL_M4A_BYTES);
  });
});
