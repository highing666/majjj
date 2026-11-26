import personaAtlasMetaJson from "../../assets/atlas/personas@2x.json";
import type { PersonaId } from "../protocol/generated/PersonaId";
import type { PersonaMood } from "../protocol/generated/PersonaMood";
import {
  getPersonaMeta,
  getPersonaMoodMeta,
  personaFrameKey,
} from "../protocol/personas";

export interface PersonaAtlasFrameRect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

export interface PersonaAvatarProjection {
  readonly frameKey: string;
  readonly ariaLabel: string;
  readonly size: number;
  readonly atlasWidth: number;
  readonly atlasHeight: number;
  readonly frame: PersonaAtlasFrameRect;
  readonly backgroundSize: string;
  readonly backgroundPosition: string;
}

interface PersonaAtlasMetaShape {
  readonly frames: Record<string, { readonly frame: PersonaAtlasFrameRect }>;
  readonly meta: {
    readonly size: {
      readonly w: number;
      readonly h: number;
    };
  };
}

const ATLAS_DATA = personaAtlasMetaJson as unknown as PersonaAtlasMetaShape;

function formatCssPx(value: number): string {
  const rounded = Number(value.toFixed(4));
  return `${Object.is(rounded, -0) ? 0 : rounded}px`;
}

/**
 * Projects a `(personaId, mood, sizePx)` tuple onto the single `@2x` comic impasto
 * persona spritesheet (`personas@2x.json`), returning exact CSS background-crop metrics.
 */
export function projectPersonaAvatar(
  personaId: PersonaId,
  mood: PersonaMood | null,
  sizePx: number,
): PersonaAvatarProjection {
  const personaMeta = getPersonaMeta(personaId);
  const resolvedMood = mood ?? personaMeta.initialMood;
  const moodMeta = getPersonaMoodMeta(resolvedMood);
  const frameKey = personaFrameKey(personaId, resolvedMood);
  const fallbackKey = personaFrameKey(personaId, personaMeta.initialMood);

  const atlasWidth = ATLAS_DATA.meta.size.w;
  const atlasHeight = ATLAS_DATA.meta.size.h;
  const entry = ATLAS_DATA.frames[frameKey] ?? ATLAS_DATA.frames[fallbackKey];
  const frame = entry.frame;

  const safeSize = Math.max(12, Math.round(sizePx));
  const scaleX = safeSize / frame.w;
  const scaleY = safeSize / frame.h;
  const backgroundWidth = atlasWidth * scaleX;
  const backgroundHeight = atlasHeight * scaleY;
  const offsetX = frame.x * scaleX;
  const offsetY = frame.y * scaleY;

  return {
    frameKey,
    ariaLabel: `${personaMeta.name} · ${moodMeta.label}`,
    size: safeSize,
    atlasWidth,
    atlasHeight,
    frame: { ...frame },
    backgroundSize: `${formatCssPx(backgroundWidth)} ${formatCssPx(backgroundHeight)}`,
    backgroundPosition: `${formatCssPx(-offsetX)} ${formatCssPx(-offsetY)}`,
  };
}
