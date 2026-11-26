//! Canonical built-in table persona (`牌友角色`) and discrete mood (`牌友情绪`) contracts.

use mahjong_ai::AiStyle;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Canonical identifier for the five built-in sharp-tongued table personas (`牌友角色`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum PersonaId {
    /// 急眼快攻哥（默认快攻牌风）
    QuickHothead,
    /// 阴阳茶艺师（默认防守做大牌牌风）
    TeaSarcasm,
    /// 嘴硬死磕王（默认快攻牌风）
    StubbornBlamer,
    /// 拱火乐子人（默认均衡牌风）
    ChaosInstigator,
    /// 红温老雀神（默认防守做大牌牌风）
    TiltedMaster,
}

/// Six discrete facial/emotional states (`牌友情绪`) driven by the presentation layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum PersonaMood {
    /// 得意欠揍
    Smug,
    /// 阴阳嘲讽
    Mocking,
    /// 红温破防
    Tilted,
    /// 心虚紧绷
    Nervous,
    /// 阴险憋大招
    Plotting,
    /// 无语装淡定
    #[default]
    Deadpan,
}

/// Canonical voice pack identifier (`角色语音包`) for the bright male human voice.
pub const HUMAN_VOICE_PACK_MALE: &str = "human_male";

/// Canonical voice pack identifier (`角色语音包`) for the crisp female human voice.
pub const HUMAN_VOICE_PACK_FEMALE: &str = "human_female";

/// All selectable human `VoicePack` (`角色语音包`) identifiers in canonical order.
pub const HUMAN_VOICE_PACK_IDS: [&str; 2] = [HUMAN_VOICE_PACK_MALE, HUMAN_VOICE_PACK_FEMALE];

/// Returns `true` when `id` is one of the two selectable human `VoicePack` identifiers.
#[must_use]
pub fn is_valid_human_voice_pack_id(id: &str) -> bool {
    HUMAN_VOICE_PACK_IDS.contains(&id)
}

/// Static contract metadata for a built-in `PersonaId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonaSpec {
    /// Canonical persona enum variant.
    pub id: PersonaId,
    /// Authoritative Chinese table nickname.
    pub display_name: &'static str,
    /// Bound pure-CPU `AiStyle` used by `mahjong-ai::decide`.
    pub default_style: AiStyle,
    /// Initial `PersonaMood` assigned when a match starts.
    pub initial_mood: PersonaMood,
    /// Short personality tagline for lobby/UI display.
    pub tagline: &'static str,
    /// Canonical offline `VoicePack` (`角色语音包`) identifier.
    pub voice_pack_id: &'static str,
}

impl PersonaId {
    /// All five built-in personas in canonical display order.
    pub const ALL: [Self; 5] = [
        Self::QuickHothead,
        Self::TeaSarcasm,
        Self::StubbornBlamer,
        Self::ChaosInstigator,
        Self::TiltedMaster,
    ];

    /// Returns the static contract specification for this persona.
    #[must_use]
    pub const fn spec(self) -> PersonaSpec {
        match self {
            Self::QuickHothead => PersonaSpec {
                id: Self::QuickHothead,
                display_name: "急眼快攻哥",
                default_style: AiStyle::AggressiveFast,
                initial_mood: PersonaMood::Smug,
                tagline: "嘴快手快，0番也狂吹",
                voice_pack_id: "quick_hothead",
            },
            Self::TeaSarcasm => PersonaSpec {
                id: Self::TeaSarcasm,
                display_name: "阴阳茶艺师",
                default_style: AiStyle::DefensiveBigHand,
                initial_mood: PersonaMood::Mocking,
                tagline: "笑里藏刀，专爱翻旧账",
                voice_pack_id: "tea_sarcasm",
            },
            Self::StubbornBlamer => PersonaSpec {
                id: Self::StubbornBlamer,
                display_name: "嘴硬死磕王",
                default_style: AiStyle::AggressiveFast,
                initial_mood: PersonaMood::Deadpan,
                tagline: "死磕到底，点炮全怪发牌员",
                voice_pack_id: "stubborn_blamer",
            },
            Self::ChaosInstigator => PersonaSpec {
                id: Self::ChaosInstigator,
                display_name: "拱火乐子人",
                default_style: AiStyle::Balanced,
                initial_mood: PersonaMood::Plotting,
                tagline: "专业拱火，最爱补刀拆台",
                voice_pack_id: "chaos_instigator",
            },
            Self::TiltedMaster => PersonaSpec {
                id: Self::TiltedMaster,
                display_name: "红温老雀神",
                default_style: AiStyle::DefensiveBigHand,
                initial_mood: PersonaMood::Smug,
                tagline: "自封牌桌导师，一输就破防",
                voice_pack_id: "tilted_master",
            },
        }
    }

    /// Returns the authoritative Chinese nickname for this persona.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        self.spec().display_name
    }

    /// Returns the default pure-CPU `AiStyle` bound to this persona.
    #[must_use]
    pub const fn default_style(self) -> AiStyle {
        self.spec().default_style
    }

    /// Returns the initial `PersonaMood` when a match begins.
    #[must_use]
    pub const fn initial_mood(self) -> PersonaMood {
        self.spec().initial_mood
    }

    /// Returns the canonical offline `VoicePack` (`角色语音包`) identifier for this persona.
    #[must_use]
    pub const fn voice_pack_id(self) -> &'static str {
        self.spec().voice_pack_id
    }
}

impl PersonaMood {
    /// All six canonical mood variants in atlas column order.
    pub const ALL: [Self; 6] = [
        Self::Smug,
        Self::Mocking,
        Self::Tilted,
        Self::Nervous,
        Self::Plotting,
        Self::Deadpan,
    ];
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use mahjong_ai::AiStyle;

    use super::{PersonaId, PersonaMood};

    #[test]
    fn five_builtin_personas_have_unique_names_and_spec_bound_styles() {
        assert_eq!(PersonaId::ALL.len(), 5);
        let names: HashSet<&str> = PersonaId::ALL.iter().map(|p| p.display_name()).collect();
        assert_eq!(names.len(), 5);

        assert_eq!(PersonaId::QuickHothead.display_name(), "急眼快攻哥");
        assert_eq!(
            PersonaId::QuickHothead.default_style(),
            AiStyle::AggressiveFast
        );
        assert_eq!(PersonaId::TeaSarcasm.display_name(), "阴阳茶艺师");
        assert_eq!(
            PersonaId::TeaSarcasm.default_style(),
            AiStyle::DefensiveBigHand
        );
        assert_eq!(PersonaId::StubbornBlamer.display_name(), "嘴硬死磕王");
        assert_eq!(
            PersonaId::StubbornBlamer.default_style(),
            AiStyle::AggressiveFast
        );
        assert_eq!(PersonaId::ChaosInstigator.display_name(), "拱火乐子人");
        assert_eq!(
            PersonaId::ChaosInstigator.default_style(),
            AiStyle::Balanced
        );
        assert_eq!(PersonaId::TiltedMaster.display_name(), "红温老雀神");
        assert_eq!(
            PersonaId::TiltedMaster.default_style(),
            AiStyle::DefensiveBigHand
        );
    }

    #[test]
    fn six_moods_serialize_as_exact_pascal_case_variants() {
        let serialized: Vec<String> = PersonaMood::ALL
            .iter()
            .map(|m| serde_json::to_string(m).expect("serialize mood"))
            .collect();
        assert_eq!(
            serialized,
            vec![
                "\"Smug\"",
                "\"Mocking\"",
                "\"Tilted\"",
                "\"Nervous\"",
                "\"Plotting\"",
                "\"Deadpan\"",
            ]
        );
    }
}
