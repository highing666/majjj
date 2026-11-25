import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const outDir = path.join(webRoot, "src/assets/audio");
const outM4a = path.join(outDir, "sfx-sprite.m4a");
const outJson = path.join(outDir, "sfx-sprite.json");

const SAMPLE_RATE = 44100;
const CLIP_GAP_SEC = 0.1;
const MAX_DURATION_SEC = 15;
const MAX_BYTES = 307200; // 300 KiB

function createRng(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function addModalMode(buf, offsetSec, durSec, freq, amp, decayRate, phase = 0) {
  const startIdx = Math.max(0, Math.floor(offsetSec * SAMPLE_RATE));
  const count = Math.floor(durSec * SAMPLE_RATE);
  const attackSamples = Math.max(1, Math.floor(0.0015 * SAMPLE_RATE));
  for (let i = 0; i < count && startIdx + i < buf.length; i += 1) {
    const t = i / SAMPLE_RATE;
    const attack = i < attackSamples ? i / attackSamples : 1;
    const env = attack * Math.exp(-decayRate * t);
    buf[startIdx + i] += amp * env * Math.sin(2 * Math.PI * freq * t + phase);
  }
}

function addPitchSweep(buf, offsetSec, durSec, fStart, fEnd, amp, decayRate, shape = "sine") {
  const startIdx = Math.max(0, Math.floor(offsetSec * SAMPLE_RATE));
  const count = Math.floor(durSec * SAMPLE_RATE);
  const attackSamples = Math.max(1, Math.floor(0.002 * SAMPLE_RATE));
  let phase = 0;
  for (let i = 0; i < count && startIdx + i < buf.length; i += 1) {
    const t = i / SAMPLE_RATE;
    const u = Math.min(1, t / Math.max(1e-4, durSec));
    const freq = fStart * Math.pow(fEnd / fStart, u);
    phase += (2 * Math.PI * freq) / SAMPLE_RATE;
    const attack = i < attackSamples ? i / attackSamples : 1;
    const env = attack * Math.exp(-decayRate * t);
    let sample = Math.sin(phase);
    if (shape === "triangle") {
      sample = (2 / Math.PI) * Math.asin(sample);
    } else if (shape === "brass") {
      sample = 0.65 * Math.sin(phase) + 0.25 * Math.sin(2 * phase) + 0.1 * Math.sin(3 * phase);
    }
    buf[startIdx + i] += amp * env * sample;
  }
}

function addBandpassNoise(buf, offsetSec, durSec, centerFreq, q, amp, decayRate, rng) {
  const startIdx = Math.max(0, Math.floor(offsetSec * SAMPLE_RATE));
  const count = Math.floor(durSec * SAMPLE_RATE);
  const w0 = (2 * Math.PI * centerFreq) / SAMPLE_RATE;
  const alpha = Math.sin(w0) / (2 * q);
  const cosW0 = Math.cos(w0);
  const b0 = alpha;
  const b1 = 0;
  const b2 = -alpha;
  const a0 = 1 + alpha;
  const a1 = -2 * cosW0;
  const a2 = 1 - alpha;

  let x1 = 0;
  let x2 = 0;
  let y1 = 0;
  let y2 = 0;
  const attackSamples = Math.max(1, Math.floor(0.0015 * SAMPLE_RATE));

  for (let i = 0; i < count && startIdx + i < buf.length; i += 1) {
    const x0 = rng() * 2 - 1;
    const y0 = (b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
    x2 = x1;
    x1 = x0;
    y2 = y1;
    y1 = y0;

    const t = i / SAMPLE_RATE;
    const attack = i < attackSamples ? i / attackSamples : 1;
    const env = attack * Math.exp(-decayRate * t);
    buf[startIdx + i] += amp * env * y0;
  }
}

function synthesizeBoneClack(buf, durSec, pitchScale, heavy, rng) {
  // Ceramic/bone resonances + wooden felt thud
  addModalMode(buf, 0, durSec, 2450 * pitchScale, heavy ? 0.34 : 0.22, 72);
  addModalMode(buf, 0, durSec, 1520 * pitchScale, heavy ? 0.28 : 0.2, 58);
  addModalMode(buf, 0.001, durSec, 3680 * pitchScale, heavy ? 0.16 : 0.1, 95);
  addBandpassNoise(buf, 0, Math.min(0.055, durSec), 2100 * pitchScale, 1.5, heavy ? 0.42 : 0.24, 65, rng);

  if (heavy) {
    addPitchSweep(buf, 0.003, durSec, 210 * pitchScale, 105 * pitchScale, 0.55, 28, "sine");
    addPitchSweep(buf, 0.002, durSec * 0.7, 780 * pitchScale, 420 * pitchScale, 0.3, 42, "triangle");
  } else {
    addPitchSweep(buf, 0.002, durSec, 290 * pitchScale, 175 * pitchScale, 0.28, 38, "sine");
  }
}

function finalizeClip(buf) {
  // Smooth 4ms fade-out at end to eliminate DC/click boundaries
  const fadeSamples = Math.min(buf.length, Math.floor(0.004 * SAMPLE_RATE));
  for (let i = 0; i < fadeSamples; i += 1) {
    const idx = buf.length - 1 - i;
    buf[idx] *= i / fadeSamples;
  }
  let peak = 0;
  for (let i = 0; i < buf.length; i += 1) {
    const abs = Math.abs(buf[i]);
    if (abs > peak) peak = abs;
  }
  if (peak > 0.88) {
    const scale = 0.88 / peak;
    for (let i = 0; i < buf.length; i += 1) {
      buf[i] *= scale;
    }
  }
  return buf;
}

function renderClip(durSec, builder) {
  const buf = new Float32Array(Math.floor(durSec * SAMPLE_RATE));
  builder(buf);
  return finalizeClip(buf);
}

function buildSlotSpecs() {
  const rng = createRng(0x4d414a4a);

  return [
    {
      slot: "hover",
      variants: [
        renderClip(0.05, (buf) => {
          addModalMode(buf, 0, 0.045, 1980, 0.22, 110);
          addModalMode(buf, 0, 0.04, 3120, 0.12, 140);
          addBandpassNoise(buf, 0, 0.025, 2400, 1.8, 0.1, 130, rng);
        }),
      ],
    },
    {
      slot: "select",
      variants: [
        renderClip(0.07, (buf) => {
          addPitchSweep(buf, 0, 0.05, 1380, 1840, 0.24, 55, "triangle");
          addModalMode(buf, 0, 0.06, 2640, 0.2, 85);
          addBandpassNoise(buf, 0, 0.03, 2800, 1.6, 0.16, 95, rng);
        }),
      ],
    },
    {
      slot: "action_prompt",
      variants: [
        renderClip(0.18, (buf) => {
          addModalMode(buf, 0, 0.16, 784.0, 0.3, 18);
          addModalMode(buf, 0.04, 0.14, 1174.66, 0.34, 16);
          addModalMode(buf, 0.04, 0.14, 2349.32, 0.1, 24);
        }),
      ],
    },
    {
      slot: "pass",
      variants: [
        renderClip(0.08, (buf) => {
          addPitchSweep(buf, 0, 0.07, 460, 290, 0.28, 48, "triangle");
          addBandpassNoise(buf, 0, 0.04, 950, 1.5, 0.14, 70, rng);
        }),
      ],
    },
    {
      slot: "draw",
      variants: [0.97, 1.0, 1.035].map((scale) =>
        renderClip(0.1, (buf) => {
          addBandpassNoise(buf, 0, 0.065, 2250 * scale, 1.4, 0.26, 44, rng);
          addModalMode(buf, 0.024, 0.065, 1280 * scale, 0.22, 65);
          addModalMode(buf, 0.026, 0.055, 2650 * scale, 0.12, 88);
        }),
      ),
    },
    {
      slot: "discard_light",
      variants: [0.965, 1.0, 1.04].map((scale) =>
        renderClip(0.12, (buf) => {
          synthesizeBoneClack(buf, 0.11, scale, false, rng);
        }),
      ),
    },
    {
      slot: "discard_heavy",
      variants: [0.96, 1.0, 1.04].map((scale) =>
        renderClip(0.15, (buf) => {
          synthesizeBoneClack(buf, 0.145, scale, true, rng);
        }),
      ),
    },
    {
      slot: "meld_pon",
      variants: [0.985, 1.02].map((scale) =>
        renderClip(0.35, (buf) => {
          const hit1 = renderClip(0.14, (b) => synthesizeBoneClack(b, 0.13, scale, true, rng));
          const hit2 = renderClip(0.14, (b) => synthesizeBoneClack(b, 0.13, scale * 1.06, true, rng));
          buf.set(hit1.subarray(0, Math.min(hit1.length, buf.length)), 0);
          const off = Math.floor(0.068 * SAMPLE_RATE);
          for (let i = 0; i < hit2.length && off + i < buf.length; i += 1) {
            buf[off + i] += hit2[i];
          }
          addPitchSweep(buf, 0.01, 0.28, 330 * scale, 495 * scale, 0.34, 11, "brass");
        }),
      ),
    },
    {
      slot: "meld_kong",
      variants: [
        renderClip(0.48, (buf) => {
          for (let idx = 0; idx < 3; idx += 1) {
            const hit = renderClip(0.14, (b) =>
              synthesizeBoneClack(b, 0.13, 0.96 + idx * 0.04, true, rng),
            );
            const off = Math.floor(idx * 0.058 * SAMPLE_RATE);
            for (let i = 0; i < hit.length && off + i < buf.length; i += 1) {
              buf[off + i] += hit[i] * 0.85;
            }
          }
          addPitchSweep(buf, 0, 0.36, 155, 48, 0.65, 9.5, "sine");
          addModalMode(buf, 0.04, 0.42, 1046.5, 0.26, 7.5);
          addModalMode(buf, 0.04, 0.42, 1567.98, 0.16, 9);
        }),
      ],
    },
    {
      slot: "added_kong_suspense",
      variants: [
        renderClip(0.3, (buf) => {
          addModalMode(buf, 0, 0.28, 987.77, 0.28, 9.5);
          addModalMode(buf, 0.03, 0.26, 1396.91, 0.24, 9.5);
          addModalMode(buf, 0.05, 0.24, 2093.0, 0.14, 12);
          addBandpassNoise(buf, 0, 0.12, 3200, 2.5, 0.16, 22, rng);
        }),
      ],
    },
    {
      slot: "win_tier_1",
      variants: [
        renderClip(0.75, (buf) => {
          synthesizeBoneClack(buf, 0.15, 1.0, true, rng);
          const notes = [261.63, 329.63, 392.0];
          notes.forEach((freq, idx) => {
            addPitchSweep(buf, idx * 0.055, 0.55, freq, freq * 1.003, 0.26, 4.8, idx === 0 ? "triangle" : "sine");
          });
        }),
      ],
    },
    {
      slot: "win_tier_2",
      variants: [
        renderClip(1.0, (buf) => {
          synthesizeBoneClack(buf, 0.16, 1.02, true, rng);
          addPitchSweep(buf, 0, 0.32, 140, 56, 0.48, 10, "sine");
          const notes = [261.63, 329.63, 392.0, 523.25, 659.25];
          notes.forEach((freq, idx) => {
            addPitchSweep(buf, idx * 0.05, 0.72, freq, freq * 1.004, 0.24, 3.9, idx < 2 ? "brass" : "sine");
          });
        }),
      ],
    },
    {
      slot: "win_tier_3",
      variants: [
        renderClip(1.35, (buf) => {
          synthesizeBoneClack(buf, 0.16, 1.05, true, rng);
          addPitchSweep(buf, 0, 0.45, 165, 44, 0.68, 7.5, "sine");
          const notes = [261.63, 329.63, 392.0, 523.25, 659.25, 783.99, 1046.5];
          notes.forEach((freq, idx) => {
            addPitchSweep(buf, idx * 0.048, 0.95, freq, freq * 1.005, 0.22, 2.9, idx < 3 ? "brass" : "sine");
            addModalMode(buf, idx * 0.048, 0.85, freq * 2, 0.07, 4.2);
          });
        }),
      ],
    },
    {
      slot: "draw_game",
      variants: [
        renderClip(0.45, (buf) => {
          addPitchSweep(buf, 0, 0.42, 215, 138, 0.45, 7.5, "triangle");
          addModalMode(buf, 0, 0.38, 392, 0.2, 9.5);
          addBandpassNoise(buf, 0, 0.14, 820, 1.6, 0.18, 18, rng);
        }),
      ],
    },
    {
      slot: "confirm",
      variants: [
        renderClip(0.2, (buf) => {
          addModalMode(buf, 0, 0.14, 523.25, 0.28, 18);
          addModalMode(buf, 0.045, 0.15, 659.25, 0.3, 15);
        }),
      ],
    },
  ];
}

function encodeMonoWav(samples, sampleRate) {
  const numSamples = samples.length;
  const dataBytes = numSamples * 2;
  const buf = Buffer.alloc(44 + dataBytes);

  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + dataBytes, 4);
  buf.write("WAVE", 8);
  buf.write("fmt ", 12);
  buf.writeUInt32LE(16, 16); // PCM chunk size
  buf.writeUInt16LE(1, 20); // PCM format
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(sampleRate, 24);
  buf.writeUInt32LE(sampleRate * 2, 28); // byte rate
  buf.writeUInt16LE(2, 32); // block align
  buf.writeUInt16LE(16, 34); // bits per sample
  buf.write("data", 36);
  buf.writeUInt32LE(dataBytes, 40);

  for (let i = 0; i < numSamples; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    buf.writeInt16LE(Math.round(clamped * 32767), 44 + i * 2);
  }
  return buf;
}

function roundSec(val) {
  return Math.round(val * 10000) / 10000;
}

function main() {
  fs.mkdirSync(outDir, { recursive: true });
  const specs = buildSlotSpecs();

  let cursorSec = 0;
  const slotsManifest = {};
  const chunks = [];

  for (const spec of specs) {
    slotsManifest[spec.slot] = [];
    for (const clipBuf of spec.variants) {
      const startSec = roundSec(cursorSec);
      const durSec = roundSec(clipBuf.length / SAMPLE_RATE);
      slotsManifest[spec.slot].push([startSec, durSec]);
      chunks.push(clipBuf);
      const gapBuf = new Float32Array(Math.floor(CLIP_GAP_SEC * SAMPLE_RATE));
      chunks.push(gapBuf);
      cursorSec += clipBuf.length / SAMPLE_RATE + CLIP_GAP_SEC;
    }
  }

  const totalSamples = chunks.reduce((sum, c) => sum + c.length, 0);
  const master = new Float32Array(totalSamples);
  let writeOffset = 0;
  for (const c of chunks) {
    master.set(c, writeOffset);
    writeOffset += c.length;
  }

  const durationSec = roundSec(totalSamples / SAMPLE_RATE);
  if (durationSec >= MAX_DURATION_SEC) {
    throw new Error(`AudioSprite duration ${durationSec}s >= ${MAX_DURATION_SEC}s limit`);
  }

  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "majjj-sfx-"));
  const tempWav = path.join(tempDir, "sfx-sprite.wav");
  try {
    fs.writeFileSync(tempWav, encodeMonoWav(master, SAMPLE_RATE));
    execFileSync("/usr/bin/afconvert", [
      "-f",
      "m4af",
      "-d",
      "aac",
      "-b",
      "96000",
      tempWav,
      outM4a,
    ]);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }

  const stat = fs.statSync(outM4a);
  if (stat.size > MAX_BYTES) {
    throw new Error(`AudioSprite size ${stat.size} bytes exceeds ${MAX_BYTES} bytes limit`);
  }

  const afinfoOut = execFileSync("/usr/bin/afinfo", [outM4a], { encoding: "utf8" });
  const match = afinfoOut.match(/estimated duration:\s*([0-9.]+)\s*sec/);
  const encodedDuration = match ? Number(match[1]) : durationSec;
  if (!(encodedDuration > 0 && encodedDuration < MAX_DURATION_SEC)) {
    throw new Error(`Encoded AudioSprite duration ${encodedDuration}s invalid or >= ${MAX_DURATION_SEC}s`);
  }

  const manifest = {
    version: 1,
    sampleRate: SAMPLE_RATE,
    duration: durationSec,
    slots: slotsManifest,
  };
  fs.writeFileSync(outJson, JSON.stringify(manifest, null, 2) + "\n");

  console.log(
    `Built sfx-sprite.m4a (${stat.size} bytes, ${encodedDuration.toFixed(3)}s, ${Object.keys(slotsManifest).length} slots)`,
  );
}

main();
