import type {
  AudioBufferLike,
  AudioContextLike,
  AudioNodeLike,
  SfxSlotId,
} from "./audioTypes";

export interface ProceduralSfx {
  playSlot(
    ctx: AudioContextLike,
    target: AudioNodeLike,
    slot: SfxSlotId,
    playbackRate?: number,
  ): void;
  dispose(): void;
}

function buildNoiseBuffer(ctx: AudioContextLike): AudioBufferLike {
  const length = Math.max(1, Math.floor(ctx.sampleRate * 0.25));
  const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i += 1) {
    data[i] = Math.random() * 2 - 1;
  }
  return buffer;
}

function playTone(
  ctx: AudioContextLike,
  target: AudioNodeLike,
  time: number,
  freqStart: number,
  freqEnd: number,
  gainVal: number,
  dur: number,
  type: OscillatorType = "triangle",
): void {
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(Math.max(20, freqStart), time);
  osc.frequency.exponentialRampToValueAtTime(
    Math.max(20, freqEnd),
    time + Math.max(0.01, dur),
  );
  gain.gain.setValueAtTime(gainVal, time);
  gain.gain.exponentialRampToValueAtTime(0.0001, time + Math.max(0.01, dur));
  osc.connect(gain);
  gain.connect(target);
  osc.start(time);
  osc.stop(time + dur + 0.015);
}

function playNoiseBurst(
  ctx: AudioContextLike,
  target: AudioNodeLike,
  noiseBuffer: AudioBufferLike,
  time: number,
  filterFreq: number,
  q: number,
  gainVal: number,
  dur: number,
): void {
  const src = ctx.createBufferSource();
  src.buffer = noiseBuffer;
  const filter = ctx.createBiquadFilter();
  filter.type = "bandpass";
  filter.frequency.setValueAtTime(Math.max(40, filterFreq), time);
  filter.Q.value = q;
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(gainVal, time);
  gain.gain.exponentialRampToValueAtTime(0.0001, time + Math.max(0.01, dur));
  src.connect(filter);
  filter.connect(gain);
  gain.connect(target);
  src.start(time);
  src.stop(time + dur + 0.015);
}

function playDiscardSlam(
  ctx: AudioContextLike,
  target: AudioNodeLike,
  noiseBuffer: AudioBufferLike,
  now: number,
  pitch: number,
): void {
  playTone(ctx, target, now, 1880 * pitch, 1034 * pitch, 0.42, 0.055);
  playTone(ctx, target, now + 0.004, 860 * pitch, 473 * pitch, 0.32, 0.07);
  playTone(ctx, target, now + 0.008, 175 * pitch, 96 * pitch, 0.5, 0.11);
  playNoiseBurst(ctx, target, noiseBuffer, now, 2100 * pitch, 1.3, 0.32, 0.06);
}

function playNoteChime(
  ctx: AudioContextLike,
  target: AudioNodeLike,
  now: number,
  freqs: readonly number[],
  stepSec: number,
  durSec: number,
  gainVal: number,
  pitch: number,
): void {
  freqs.forEach((freq, idx) => {
    const start = now + idx * stepSec;
    playTone(
      ctx,
      target,
      start,
      freq * pitch,
      freq * pitch * 0.995,
      gainVal,
      durSec,
      idx < 2 ? "triangle" : "sine",
    );
  });
}

/**
 * Creates a 1:1 slot-mapped native Web Audio procedural synthesizer fallback
 * mirroring the Piltover bone/ceramic/wood SFX palette.
 */
export function createProceduralSfx(): ProceduralSfx {
  const noiseCache = new WeakMap<AudioContextLike, AudioBufferLike>();

  function getNoiseBuffer(ctx: AudioContextLike): AudioBufferLike {
    let cached = noiseCache.get(ctx);
    if (!cached) {
      cached = buildNoiseBuffer(ctx);
      noiseCache.set(ctx, cached);
    }
    return cached;
  }

  return {
    playSlot(
      ctx: AudioContextLike,
      target: AudioNodeLike,
      slot: SfxSlotId,
      playbackRate = 1,
    ): void {
      const pitch = Math.max(0.5, Math.min(2, playbackRate));
      const now = ctx.currentTime;
      const noise = getNoiseBuffer(ctx);

      switch (slot) {
        case "hover":
          playTone(ctx, target, now, 1650 * pitch, 908 * pitch, 0.04, 0.025);
          break;
        case "select":
          playTone(ctx, target, now, 1380 * pitch, 1780 * pitch, 0.14, 0.045);
          playNoiseBurst(ctx, target, noise, now, 2600 * pitch, 1.6, 0.08, 0.03);
          break;
        case "action_prompt":
          playNoteChime(ctx, target, now, [784, 1174.66], 0.04, 0.14, 0.18, pitch);
          break;
        case "pass":
          playTone(ctx, target, now, 420 * pitch, 231 * pitch, 0.16, 0.06);
          break;
        case "draw":
          playNoiseBurst(ctx, target, noise, now, 2200 * pitch, 1.4, 0.14, 0.075);
          playTone(ctx, target, now + 0.025, 1180 * pitch, 649 * pitch, 0.14, 0.04);
          break;
        case "discard_light":
          playTone(ctx, target, now, 1320 * pitch, 726 * pitch, 0.28, 0.055);
          playTone(ctx, target, now + 0.006, 260 * pitch, 143 * pitch, 0.25, 0.065);
          playNoiseBurst(ctx, target, noise, now, 1500 * pitch, 1.8, 0.18, 0.04);
          break;
        case "discard_heavy":
          playDiscardSlam(ctx, target, noise, now, pitch);
          break;
        case "meld_pon": {
          playDiscardSlam(ctx, target, noise, now, pitch);
          playDiscardSlam(ctx, target, noise, now + 0.07, pitch * 1.04);
          const osc = ctx.createOscillator();
          const lp = ctx.createBiquadFilter();
          const gain = ctx.createGain();
          osc.type = "sawtooth";
          osc.frequency.setValueAtTime(330 * pitch, now);
          osc.frequency.exponentialRampToValueAtTime(495 * pitch, now + 0.09);
          lp.type = "lowpass";
          lp.frequency.value = 1800;
          gain.gain.setValueAtTime(0.18, now);
          gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.22);
          osc.connect(lp);
          lp.connect(gain);
          gain.connect(target);
          osc.start(now);
          osc.stop(now + 0.24);
          break;
        }
        case "meld_kong":
          playDiscardSlam(ctx, target, noise, now, pitch);
          playDiscardSlam(ctx, target, noise, now + 0.06, pitch * 1.03);
          playDiscardSlam(ctx, target, noise, now + 0.12, pitch * 1.06);
          playTone(ctx, target, now, 145 * pitch, 46 * pitch, 0.62, 0.26, "sine");
          playTone(ctx, target, now + 0.04, 1046 * pitch, 575 * pitch, 0.24, 0.32);
          break;
        case "added_kong_suspense":
          playNoteChime(
            ctx,
            target,
            now,
            [987.77, 1396.91, 2093],
            0.03,
            0.22,
            0.16,
            pitch,
          );
          playNoiseBurst(ctx, target, noise, now, 3200 * pitch, 2.2, 0.1, 0.1);
          break;
        case "win_tier_1":
          playDiscardSlam(ctx, target, noise, now, pitch);
          playNoteChime(
            ctx,
            target,
            now,
            [261.63, 329.63, 392.0],
            0.05,
            0.42,
            0.2,
            pitch,
          );
          break;
        case "win_tier_2":
          playDiscardSlam(ctx, target, noise, now, pitch);
          playNoteChime(
            ctx,
            target,
            now,
            [261.63, 329.63, 392.0, 523.25, 659.25],
            0.045,
            0.55,
            0.22,
            pitch,
          );
          break;
        case "win_tier_3":
          playDiscardSlam(ctx, target, noise, now, pitch);
          playTone(ctx, target, now, 165 * pitch, 44 * pitch, 0.58, 0.35, "sine");
          playNoteChime(
            ctx,
            target,
            now,
            [261.63, 329.63, 392.0, 523.25, 659.25, 783.99, 1046.5],
            0.042,
            0.72,
            0.22,
            pitch,
          );
          break;
        case "draw_game":
          playTone(ctx, target, now, 215 * pitch, 138 * pitch, 0.36, 0.34, "triangle");
          playNoiseBurst(ctx, target, noise, now, 820 * pitch, 1.5, 0.14, 0.12);
          break;
        case "confirm":
          playNoteChime(ctx, target, now, [523.25, 659.25], 0.045, 0.16, 0.2, pitch);
          break;
      }
    },

    dispose(): void {
      // WeakMap entries are garbage-collected automatically when contexts drop.
    },
  };
}
