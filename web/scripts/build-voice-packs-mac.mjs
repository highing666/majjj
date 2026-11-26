#!/usr/bin/env node
/**
 * Local macOS playable 7-pack VoicePack (角色语音包) generator.
 *
 * Uses macOS `/usr/bin/say` (Tingting / Meijia Mandarin voices) + pure-Node PCM
 * pitch/timbre shaping, silence trimming, RMS loudness normalization, AudioSprite
 * stitching, and `/usr/bin/afconvert` mono AAC encoding to build all 7 packs
 * under `web/public/assets/vo/<pack_id>/{sprite.m4a,manifest.json}`.
 *
 * When the Windows GPU host (`BENDER`) later records 1.7B studio-quality packs
 * via `scripts/windows-tts/generate_voice_packs.py`, those `.m4a` and
 * `manifest.json` outputs drop directly into the same paths with zero code edits.
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const schemaPath = path.join(webRoot, "src/lib/audio/voicePackSchema.ts");
const defaultOutDir = path.join(webRoot, "public/assets/vo");

const SAMPLE_RATE = 24000;
const DEFAULT_BITRATE_BPS = 32000;
const TARGET_RMS_DBFS = -16.0;
const PEAK_LIMIT_DBFS = -1.5;
const CLIP_GAP_SEC = 0.08;
const TAIL_PAD_SEC = 0.08;
const MAX_TOTAL_M4A_BYTES = 2 * 1024 * 1024; // 2.0 MiB hard budget ceiling

/**
 * Distinct Mandarin vocal profiles across the 7 canonical packs.
 * Pitch resampling shifts vocal tract & fundamental pitch while compensating
 * speech rate so every pack stays intelligible, crisp, and immediately distinct.
 */
const VOICE_PACK_PROFILES = [
  {
    packId: "quick_hothead",
    label: "急眼快攻哥",
    voice: "Tingting",
    rate: 255,
    pitchRatio: 0.84,
    highPassHz: 85,
    lowPassHz: 6800,
  },
  {
    packId: "tea_sarcasm",
    label: "阴阳茶艺师",
    voice: "Meijia",
    rate: 182,
    pitchRatio: 1.0,
    highPassHz: 95,
    lowPassHz: 7500,
  },
  {
    packId: "stubborn_blamer",
    label: "嘴硬死磕王",
    voice: "Tingting",
    rate: 234,
    pitchRatio: 0.8,
    highPassHz: 80,
    lowPassHz: 6200,
  },
  {
    packId: "chaos_instigator",
    label: "拱火乐子人",
    voice: "Tingting",
    rate: 208,
    pitchRatio: 1.08,
    highPassHz: 110,
    lowPassHz: 8000,
  },
  {
    packId: "tilted_master",
    label: "红温老雀神",
    voice: "Tingting",
    rate: 216,
    pitchRatio: 0.74,
    highPassHz: 70,
    lowPassHz: 5200,
  },
  {
    packId: "human_male",
    label: "清亮男声",
    voice: "Meijia",
    rate: 232,
    pitchRatio: 0.85,
    highPassHz: 85,
    lowPassHz: 7200,
  },
  {
    packId: "human_female",
    label: "利落女声",
    voice: "Tingting",
    rate: 200,
    pitchRatio: 1.02,
    highPassHz: 100,
    lowPassHz: 7800,
  },
];

function loadCanonicalSlotsFromSchema() {
  const src = fs.readFileSync(schemaPath, "utf8");
  const match = src.match(
    /export const VOICE_SLOT_SPEECH_TEXT:\s*Record<VoiceSlotId,\s*string>\s*=\s*\{([\s\S]*?)\};/,
  );
  if (!match) {
    throw new Error(`Could not locate VOICE_SLOT_SPEECH_TEXT in ${schemaPath}`);
  }
  const slots = [];
  const regex = /([a-z0-9_]+)\s*:\s*"([^"]+)"/g;
  let m;
  while ((m = regex.exec(match[1])) !== null) {
    slots.push({ slotId: m[1], text: m[2] });
  }
  if (slots.length !== 46) {
    throw new Error(`Expected 46 canonical VoicePack slots in schema, parsed ${slots.length}`);
  }
  return slots;
}

/**
 * Parse a RIFF/WAVE file by walking subchunks (`fmt `, `data`) so padded headers
 * like macOS `say`'s `FLLR` chunk are handled accurately.
 */
function readMonoPcm16Wav(filePath) {
  const buf = fs.readFileSync(filePath);
  if (buf.length < 12 || buf.toString("ascii", 0, 4) !== "RIFF" || buf.toString("ascii", 8, 12) !== "WAVE") {
    throw new Error(`Invalid WAV file header: ${filePath}`);
  }

  let sampleRate = SAMPLE_RATE;
  let channels = 1;
  let bitsPerSample = 16;
  let dataStart = -1;
  let dataSize = 0;

  let pos = 12;
  while (pos + 8 <= buf.length) {
    const id = buf.toString("ascii", pos, pos + 4);
    const size = buf.readUInt32LE(pos + 4);
    const bodyStart = pos + 8;
    if (bodyStart + size > buf.length) {
      break;
    }
    if (id === "fmt ") {
      channels = buf.readUInt16LE(bodyStart + 2);
      sampleRate = buf.readUInt32LE(bodyStart + 4);
      bitsPerSample = buf.readUInt16LE(bodyStart + 14);
    } else if (id === "data") {
      dataStart = bodyStart;
      dataSize = size;
      break;
    }
    pos = bodyStart + size + (size % 2);
  }

  if (dataStart < 0 || dataSize <= 0) {
    throw new Error(
      `WAV contains zero PCM audio frames (${filePath}). If running inside a restricted sandbox, run outside sandbox so macOS speechsynthesisd can synthesize speech.`,
    );
  }
  if (channels !== 1 || bitsPerSample !== 16) {
    throw new Error(`Expected mono 16-bit WAV, got ${channels}ch ${bitsPerSample}-bit`);
  }

  const numSamples = Math.floor(dataSize / 2);
  const out = new Float32Array(numSamples);
  for (let i = 0; i < numSamples; i += 1) {
    out[i] = buf.readInt16LE(dataStart + i * 2) / 32767;
  }
  return { samples: out, sampleRate };
}

function encodeMonoWav(samples, sampleRate) {
  const numSamples = samples.length;
  const dataBytes = numSamples * 2;
  const buf = Buffer.alloc(44 + dataBytes);

  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + dataBytes, 4);
  buf.write("WAVE", 8);
  buf.write("fmt ", 12);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(sampleRate, 24);
  buf.writeUInt32LE(sampleRate * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write("data", 36);
  buf.writeUInt32LE(dataBytes, 40);

  for (let i = 0; i < numSamples; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    buf.writeInt16LE(Math.round(clamped * 32767), 44 + i * 2);
  }
  return buf;
}

function resamplePitch(samples, pitchRatio) {
  if (Math.abs(pitchRatio - 1.0) < 1e-4) {
    return Float32Array.from(samples);
  }
  const outLen = Math.max(1, Math.round(samples.length / pitchRatio));
  const out = new Float32Array(outLen);
  const lastIdx = samples.length - 1;
  for (let i = 0; i < outLen; i += 1) {
    const srcPos = i * pitchRatio;
    const lo = Math.floor(srcPos);
    if (lo >= lastIdx) {
      out[i] = samples[lastIdx];
    } else {
      const frac = srcPos - lo;
      out[i] = samples[lo] * (1 - frac) + samples[lo + 1] * frac;
    }
  }
  return out;
}

function applyToneShaping(samples, sampleRate, highPassHz, lowPassHz) {
  // 1-pole high-pass removes rumble/DC; 1-pole low-pass shapes vocal warmth.
  const out = new Float32Array(samples.length);
  const rcHp = 1 / (2 * Math.PI * highPassHz);
  const dt = 1 / sampleRate;
  const alphaHp = rcHp / (rcHp + dt);

  let prevX = 0;
  let prevHp = 0;
  for (let i = 0; i < samples.length; i += 1) {
    const x = samples[i];
    const hp = alphaHp * (prevHp + x - prevX);
    prevX = x;
    prevHp = hp;
    out[i] = hp;
  }

  const rcLp = 1 / (2 * Math.PI * lowPassHz);
  const alphaLp = dt / (rcLp + dt);
  let prevLp = out[0] ?? 0;
  for (let i = 0; i < out.length; i += 1) {
    prevLp += alphaLp * (out[i] - prevLp);
    out[i] = prevLp;
  }
  return out;
}

function trimSilenceAndFade(samples, sampleRate) {
  let peak = 0;
  for (let i = 0; i < samples.length; i += 1) {
    const a = Math.abs(samples[i]);
    if (a > peak) peak = a;
  }
  if (peak < 1e-4) {
    throw new Error("Synthesized clip is silent (peak < 1e-4)");
  }

  const threshold = Math.max(Math.pow(10, -40 / 20), peak * 0.018);
  const win = Math.max(1, Math.round(sampleRate * 0.005));

  let firstActive = 0;
  for (let i = 0; i < samples.length; i += win) {
    let segPeak = 0;
    const end = Math.min(samples.length, i + win);
    for (let j = i; j < end; j += 1) {
      const a = Math.abs(samples[j]);
      if (a > segPeak) segPeak = a;
    }
    if (segPeak >= threshold) {
      firstActive = i;
      break;
    }
  }

  let lastActive = samples.length;
  for (let i = samples.length; i > 0; i -= win) {
    const start = Math.max(0, i - win);
    let segPeak = 0;
    for (let j = start; j < i; j += 1) {
      const a = Math.abs(samples[j]);
      if (a > segPeak) segPeak = a;
    }
    if (segPeak >= threshold) {
      lastActive = i;
      break;
    }
  }

  const headPad = Math.round(sampleRate * 0.015);
  const tailPad = Math.round(sampleRate * 0.025);
  const cropStart = Math.max(0, firstActive - headPad);
  const cropEnd = Math.min(samples.length, lastActive + tailPad);
  const cropped = samples.slice(cropStart, Math.max(cropStart + 1, cropEnd));

  const fadeSamples = Math.min(Math.floor(cropped.length / 2), Math.max(1, Math.round(sampleRate * 0.004)));
  for (let i = 0; i < fadeSamples; i += 1) {
    const g = i / fadeSamples;
    cropped[i] *= g;
    cropped[cropped.length - 1 - i] *= g;
  }
  return cropped;
}

function normalizeLoudness(samples) {
  let sumSq = 0;
  let peak = 0;
  for (let i = 0; i < samples.length; i += 1) {
    const s = samples[i];
    sumSq += s * s;
    const a = Math.abs(s);
    if (a > peak) peak = a;
  }
  const rms = Math.sqrt(sumSq / Math.max(1, samples.length));
  if (rms < 1e-6 || peak < 1e-6) {
    throw new Error("Cannot loudness-normalize silent clip");
  }

  const targetRms = Math.pow(10, TARGET_RMS_DBFS / 20);
  const peakCeiling = Math.pow(10, PEAK_LIMIT_DBFS / 20);
  let gain = targetRms / rms;
  if (peak * gain > peakCeiling) {
    gain = peakCeiling / peak;
  }

  const out = new Float32Array(samples.length);
  for (let i = 0; i < samples.length; i += 1) {
    out[i] = Math.max(-1, Math.min(1, samples[i] * gain));
  }
  return out;
}

function roundSec(val) {
  return Math.round(val * 10000) / 10000;
}

function synthesizeSlotWithSay(profile, text, tmpWavPath) {
  // Strip exclamation/question punctuation for clean say pronunciation while preserving cadence.
  const spoken = text.replace(/[！!？?]/g, "").trim();
  execFileSync("/usr/bin/say", [
    "-v",
    profile.voice,
    "-r",
    String(profile.rate),
    "-o",
    tmpWavPath,
    "--file-format=WAVE",
    `--data-format=LEI16@${SAMPLE_RATE}`,
    spoken,
  ]);
  const { samples, sampleRate } = readMonoPcm16Wav(tmpWavPath);
  const pitched = resamplePitch(samples, profile.pitchRatio);
  const shaped = applyToneShaping(pitched, sampleRate, profile.highPassHz, profile.lowPassHz);
  const trimmed = trimSilenceAndFade(shaped, sampleRate);
  return normalizeLoudness(trimmed);
}

function buildPack(profile, slots, outDir, bitrateBps) {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), `majjj-vo-mac-${profile.packId}-`));
  try {
    const chunks = [];
    const sprites = {};
    const gapSamples = Math.round(CLIP_GAP_SEC * SAMPLE_RATE);
    const tailSamples = Math.round(TAIL_PAD_SEC * SAMPLE_RATE);
    const gapBuf = new Float32Array(gapSamples);
    let cursorSamples = 0;

    for (let idx = 0; idx < slots.length; idx += 1) {
      const slot = slots[idx];
      const clipWav = path.join(tmpDir, `${slot.slotId}.wav`);
      const processed = synthesizeSlotWithSay(profile, slot.text, clipWav);
      const startSec = roundSec(cursorSamples / SAMPLE_RATE);
      const durSec = roundSec(processed.length / SAMPLE_RATE);
      sprites[slot.slotId] = [startSec, durSec];
      chunks.push(processed);
      cursorSamples += processed.length;
      if (idx < slots.length - 1) {
        chunks.push(gapBuf);
        cursorSamples += gapSamples;
      }
    }

    chunks.push(new Float32Array(tailSamples));
    cursorSamples += tailSamples;

    const master = new Float32Array(cursorSamples);
    let writeOffset = 0;
    for (const c of chunks) {
      master.set(c, writeOffset);
      writeOffset += c.length;
    }

    const masterWavPath = path.join(tmpDir, "sprite.wav");
    const tempM4aPath = path.join(tmpDir, "sprite.m4a");
    fs.writeFileSync(masterWavPath, encodeMonoWav(master, SAMPLE_RATE));

    execFileSync("/usr/bin/afconvert", [
      "-f",
      "m4af",
      "-d",
      "aac",
      "-c",
      "1",
      "-b",
      String(bitrateBps),
      masterWavPath,
      tempM4aPath,
    ]);

    const afinfoOut = execFileSync("/usr/bin/afinfo", [tempM4aPath], { encoding: "utf8" });
    const match = afinfoOut.match(/estimated duration:\s*([0-9.]+)\s*sec/);
    const encodedDuration = match ? Number(match[1]) : roundSec(cursorSamples / SAMPLE_RATE);
    const pcmDuration = roundSec(cursorSamples / SAMPLE_RATE);

    if (!(encodedDuration > 0)) {
      throw new Error(`${profile.packId}: invalid encoded duration ${encodedDuration}s`);
    }
    for (const [slotId, [start, dur]] of Object.entries(sprites)) {
      if (start + dur > encodedDuration + 0.05) {
        throw new Error(
          `${profile.packId}:${slotId} slice end ${(start + dur).toFixed(4)}s exceeds encoded duration ${encodedDuration.toFixed(4)}s`,
        );
      }
    }

    const packOutDir = path.join(outDir, profile.packId);
    fs.mkdirSync(packOutDir, { recursive: true });
    const finalM4aPath = path.join(packOutDir, "sprite.m4a");
    const finalManifestPath = path.join(packOutDir, "manifest.json");

    fs.copyFileSync(tempM4aPath, finalM4aPath);
    const manifest = {
      pack_id: profile.packId,
      version: 1,
      audio: "sprite.m4a",
      sample_rate: SAMPLE_RATE,
      duration: pcmDuration,
      sprites,
    };
    fs.writeFileSync(finalManifestPath, JSON.stringify(manifest, null, 2) + "\n");

    const bytes = fs.statSync(finalM4aPath).size;
    return {
      packId: profile.packId,
      label: profile.label,
      bytes,
      pcmDuration,
      encodedDuration,
      slotCount: Object.keys(sprites).length,
    };
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

function parseCliArgs(argv) {
  let bitrate = DEFAULT_BITRATE_BPS;
  let outDir = defaultOutDir;
  let selectedPacks = null;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--bitrate" && argv[i + 1]) {
      bitrate = Number(argv[i + 1]);
      i += 1;
    } else if (arg === "--out-dir" && argv[i + 1]) {
      outDir = path.resolve(argv[i + 1]);
      i += 1;
    } else if (arg === "--packs") {
      selectedPacks = [];
      while (argv[i + 1] && !argv[i + 1].startsWith("--")) {
        selectedPacks.push(argv[i + 1]);
        i += 1;
      }
    } else {
      throw new Error(`Unknown CLI argument: ${arg}`);
    }
  }
  return { bitrate, outDir, selectedPacks };
}

function main() {
  const { bitrate, outDir, selectedPacks } = parseCliArgs(process.argv.slice(2));
  const slots = loadCanonicalSlotsFromSchema();

  const profilesById = new Map(VOICE_PACK_PROFILES.map((p) => [p.packId, p]));
  const targets = selectedPacks
    ? selectedPacks.map((id) => {
        const p = profilesById.get(id);
        if (!p) {
          throw new Error(`Unknown pack ID '${id}'; expected one of ${[...profilesById.keys()].join(", ")}`);
        }
        return p;
      })
    : VOICE_PACK_PROFILES;

  let totalBytes = 0;
  for (const profile of targets) {
    const res = buildPack(profile, slots, outDir, bitrate);
    totalBytes += res.bytes;
    console.log(
      `[build-vo-mac] ${res.packId} (${res.label}): ${res.slotCount} slots, ` +
        `${res.pcmDuration.toFixed(2)}s (m4a ${res.encodedDuration.toFixed(2)}s), ` +
        `${res.bytes.toLocaleString()} bytes`,
    );
  }

  console.log(
    `[build-vo-mac] Built ${targets.length} VoicePack(s), total .m4a size: ${totalBytes.toLocaleString()} bytes (${(totalBytes / (1024 * 1024)).toFixed(2)} MiB)`,
  );
  if (targets.length === VOICE_PACK_PROFILES.length && totalBytes > MAX_TOTAL_M4A_BYTES) {
    throw new Error(
      `Total VoicePack .m4a size (${totalBytes} bytes) exceeds 2.0 MiB budget (${MAX_TOTAL_M4A_BYTES} bytes)`,
    );
  }
}

main();
