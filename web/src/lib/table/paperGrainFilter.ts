import { Filter, GlProgram, UniformGroup } from "pixi.js";

export interface PaperGrainFilterOptions {
  /** Subtle paper grain amplitude (`0..0.1`). */
  grainStrength?: number;
  /** Cool-indigo edge vignette intensity (`0..1`). */
  vignetteStrength?: number;
  /** Deterministic spatial seed for subtle paper texture. */
  seed?: number;
  /** Initial enabled state (`false` when render quality is degraded). */
  enabled?: boolean;
}

export const DEFAULT_PAPER_GRAIN_OPTIONS = {
  grainStrength: 0.028,
  vignetteStrength: 0.24,
  seed: 13.37,
} as const;

const PAPER_GRAIN_VERTEX = `
in vec2 aPosition;
out vec2 vTextureCoord;
out vec2 vNormalizedCoord;

uniform vec4 uInputSize;
uniform vec4 uOutputFrame;
uniform vec4 uOutputTexture;

vec4 filterVertexPosition(void)
{
    vec2 position = aPosition * uOutputFrame.zw + uOutputFrame.xy;
    position.x = position.x * (2.0 / uOutputTexture.x) - 1.0;
    position.y = position.y * (2.0 * uOutputTexture.z / uOutputTexture.y) - uOutputTexture.z;
    return vec4(position, 0.0, 1.0);
}

vec2 filterTextureCoord(void)
{
    return aPosition * (uOutputFrame.zw * uInputSize.zw);
}

void main(void)
{
    gl_Position = filterVertexPosition();
    vTextureCoord = filterTextureCoord();
    vNormalizedCoord = aPosition;
}
`;

// Single-pass shader: overlays subtle paper tooth (attenuated on bright ivory/symbol faces
// to keep the 34 tile symbols crisp) and a cool-indigo (#080C1A) edge vignette.
const PAPER_GRAIN_FRAGMENT = `
in vec2 vTextureCoord;
in vec2 vNormalizedCoord;

out vec4 finalColor;

uniform sampler2D uTexture;
uniform float uGrainStrength;
uniform float uVignetteStrength;
uniform float uSeed;

float hash21(vec2 p)
{
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

void main(void)
{
    vec4 color = texture(uTexture, vTextureCoord);
    if (color.a <= 0.0) {
        finalColor = color;
        return;
    }

    vec3 rgb = color.rgb / color.a;

    // Fine static paper tooth + gentle fiber modulation, attenuated on bright ivory faces
    float fine = hash21(floor(gl_FragCoord.xy) + uSeed);
    float fiber = hash21(floor(gl_FragCoord.xy * 0.5) + uSeed * 1.7);
    float paperNoise = ((fine * 0.65 + fiber * 0.35) - 0.5) * uGrainStrength;
    float luma = dot(rgb, vec3(0.299, 0.587, 0.114));
    float symbolPreserve = mix(1.0, 0.32, smoothstep(0.58, 0.90, luma));
    rgb += paperNoise * symbolPreserve;

    // Cool-indigo Piltover edge vignette (#080C1A)
    vec2 centered = (vNormalizedCoord - 0.5) * 2.0;
    float dist = length(centered * vec2(0.92, 1.0));
    float vig = smoothstep(0.62, 1.42, dist) * uVignetteStrength;
    rgb = mix(rgb, vec3(0.0314, 0.0471, 0.1020), vig);

    rgb = clamp(rgb, 0.0, 1.0);
    finalColor = vec4(rgb * color.a, color.a);
}
`;

export class PaperGrainFilter extends Filter {
  constructor(options: PaperGrainFilterOptions = {}) {
    const grainStrength =
      options.grainStrength ?? DEFAULT_PAPER_GRAIN_OPTIONS.grainStrength;
    const vignetteStrength =
      options.vignetteStrength ?? DEFAULT_PAPER_GRAIN_OPTIONS.vignetteStrength;
    const seed = options.seed ?? DEFAULT_PAPER_GRAIN_OPTIONS.seed;

    const glProgram = GlProgram.from({
      vertex: PAPER_GRAIN_VERTEX,
      fragment: PAPER_GRAIN_FRAGMENT,
      name: "piltover-paper-grain-filter",
    });

    super({
      glProgram,
      resolution: "inherit",
      antialias: "inherit",
      clipToViewport: true,
      resources: {
        paperGrainUniforms: new UniformGroup({
          uGrainStrength: { value: grainStrength, type: "f32" },
          uVignetteStrength: { value: vignetteStrength, type: "f32" },
          uSeed: { value: seed, type: "f32" },
        }),
      },
    });

    if (options.enabled !== undefined) {
      this.enabled = options.enabled;
    }
  }
}

/**
 * Creates a lightweight single-pass PixiJS v8 `Filter` adding Piltover paper grain
 * and a cool-indigo edge vignette while preserving tile symbol crispness.
 */
export function createPaperGrainFilter(
  options: PaperGrainFilterOptions = {},
): PaperGrainFilter {
  return new PaperGrainFilter(options);
}
