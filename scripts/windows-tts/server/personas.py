"""Canonical 5 PersonaId x 6 PersonaMood registry for Windows LAN emotional TTS."""

from __future__ import annotations

from dataclasses import dataclass
import json
import pathlib
from typing import Dict, Mapping, Optional, Tuple

CANONICAL_PERSONAS: Tuple[str, ...] = (
    "QuickHothead",
    "TeaSarcasm",
    "StubbornBlamer",
    "ChaosInstigator",
    "TiltedMaster",
)

CANONICAL_MOODS: Tuple[str, ...] = (
    "Smug",
    "Mocking",
    "Tilted",
    "Nervous",
    "Plotting",
    "Deadpan",
)

COSYVOICE_PROMPT_PREFIX = "You are a helpful assistant."
COSYVOICE_END_OF_PROMPT = "<|endofprompt|>"


class PersonaValidationError(ValueError):
    """Base error raised when persona or mood parameters violate domain contract."""


class PersonaNotFoundError(PersonaValidationError):
    """Raised when an unknown persona identifier or voice alias is requested."""


class MoodNotFoundError(PersonaValidationError):
    """Raised when an unknown mood identifier is requested."""


@dataclass(frozen=True)
class PersonaProfile:
    persona_id: str
    voice_pack_id: str
    display_name: str
    speaker: str
    language: str
    dialect_note: str
    aliases: Tuple[str, ...]


@dataclass(frozen=True)
class MoodProfile:
    mood: str
    anchor_zh: str
    instruct: str
    paralinguistic_hint: str


@dataclass(frozen=True)
class SynthesisVoicePlan:
    persona_id: str
    voice_pack_id: str
    display_name: str
    speaker: str
    language: str
    mood: str
    instruct: str
    cosyvoice_instruct: str
    paralinguistic_hint: str
    reference_key: str


def format_cosyvoice_instruct(instruct: str) -> str:
    """Format an emotion instruction for Fun-CosyVoice3-0.5B-2512."""
    cleaned = instruct.strip()
    if COSYVOICE_END_OF_PROMPT in cleaned:
        return cleaned
    return f"{COSYVOICE_PROMPT_PREFIX} {cleaned}{COSYVOICE_END_OF_PROMPT}"


class PersonaRegistry:
    """Loads and validates the 5 built-in personas and 6 canonical moods."""

    def __init__(
        self,
        personas: Mapping[str, PersonaProfile],
        moods: Mapping[str, MoodProfile],
    ) -> None:
        missing_personas = set(CANONICAL_PERSONAS) - set(personas.keys())
        if missing_personas:
            raise ValueError(f"Missing canonical personas: {sorted(missing_personas)}")
        missing_moods = set(CANONICAL_MOODS) - set(moods.keys())
        if missing_moods:
            raise ValueError(f"Missing canonical moods: {sorted(missing_moods)}")

        self._personas: Dict[str, PersonaProfile] = dict(personas)
        self._moods: Dict[str, MoodProfile] = dict(moods)
        self._alias_map: Dict[str, str] = {}
        for persona_id, profile in self._personas.items():
            for token in (
                persona_id,
                profile.voice_pack_id,
                profile.speaker,
                profile.display_name,
                *profile.aliases,
            ):
                normalized = token.strip().lower()
                if normalized:
                    self._alias_map[normalized] = persona_id

        self._mood_alias_map: Dict[str, str] = {
            mood.lower(): mood for mood in self._moods
        }

    @classmethod
    def from_file(cls, path: pathlib.Path | str) -> "PersonaRegistry":
        raw = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
        personas: Dict[str, PersonaProfile] = {}
        for persona_id, entry in raw.get("personas", {}).items():
            personas[persona_id] = PersonaProfile(
                persona_id=persona_id,
                voice_pack_id=str(entry["voice_pack_id"]),
                display_name=str(entry["display_name"]),
                speaker=str(entry["speaker"]),
                language=str(entry.get("language", "Chinese")),
                dialect_note=str(entry.get("dialect_note", "")),
                aliases=tuple(str(a) for a in entry.get("aliases", ())),
            )

        moods: Dict[str, MoodProfile] = {}
        for mood_id, entry in raw.get("moods", {}).items():
            moods[mood_id] = MoodProfile(
                mood=mood_id,
                anchor_zh=str(entry["anchor_zh"]),
                instruct=str(entry["instruct"]),
                paralinguistic_hint=str(entry.get("paralinguistic_hint", "")),
            )

        return cls(personas=personas, moods=moods)

    @property
    def persona_count(self) -> int:
        return len(self._personas)

    @property
    def mood_count(self) -> int:
        return len(self._moods)

    def list_personas(self) -> Tuple[PersonaProfile, ...]:
        return tuple(self._personas[pid] for pid in CANONICAL_PERSONAS)

    def resolve(
        self,
        *,
        persona_id: str,
        mood: str,
        instruct_override: Optional[str] = None,
    ) -> SynthesisVoicePlan:
        canonical_persona = self._alias_map.get(persona_id.strip().lower())
        if canonical_persona is None:
            raise PersonaNotFoundError(
                f"Unknown persona_id '{persona_id}'; expected one of {list(CANONICAL_PERSONAS)}"
            )

        canonical_mood = self._mood_alias_map.get(mood.strip().lower())
        if canonical_mood is None:
            raise MoodNotFoundError(
                f"Unknown mood '{mood}'; expected one of {list(CANONICAL_MOODS)}"
            )

        persona = self._personas[canonical_persona]
        mood_profile = self._moods[canonical_mood]
        effective_instruct = (
            instruct_override.strip()
            if instruct_override and instruct_override.strip()
            else mood_profile.instruct
        )

        return SynthesisVoicePlan(
            persona_id=persona.persona_id,
            voice_pack_id=persona.voice_pack_id,
            display_name=persona.display_name,
            speaker=persona.speaker,
            language=persona.language,
            mood=mood_profile.mood,
            instruct=effective_instruct,
            cosyvoice_instruct=format_cosyvoice_instruct(effective_instruct),
            paralinguistic_hint=mood_profile.paralinguistic_hint,
            reference_key=f"{persona.voice_pack_id}_{mood_profile.mood.lower()}",
        )

    def resolve_openai_voice(
        self,
        voice: str,
        *,
        mood: str = "Deadpan",
        instruct_override: Optional[str] = None,
    ) -> SynthesisVoicePlan:
        """Resolve OpenAI /v1/audio/speech voice names (persona_id, speaker, or alias)."""
        return self.resolve(
            persona_id=voice,
            mood=mood,
            instruct_override=instruct_override,
        )
