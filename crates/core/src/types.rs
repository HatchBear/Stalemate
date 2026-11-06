//! Shared types (12-programs-and-code.md, "Shared types"). Other modules import these from the crate root.

pub type AccountId = u32;
pub type HeroId = u32;
pub type ItemId = u32;
pub type MatchId = u64;
pub type GameSecs = u32;

/// Normalized parser output. Identical for haste (PRG-04) and the .NET fallback (PRG-21).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum ParserEvent {
    Roster {
        players: Vec<RosterEntry>,
    },
    Tick {
        t_game: GameSecs,
    },
    Inventory {
        account_id: AccountId,
        items: Vec<ItemId>,
    },
    Kda {
        account_id: AccountId,
        kills: u16,
        deaths: u16,
        assists: u16,
    },
    Level {
        account_id: AccountId,
        level: u8,
    },
    /// A player's net worth in souls (LS-08). From `m_iGoldNetWorth`. Whether it may reach the UI
    /// is decided by `live_safe`, not here.
    NetWorth {
        account_id: AccountId,
        souls: u32,
    },
    /// The raw game mode and match mode numbers from the Broadcast (`m_eGameMode`,
    /// `m_eMatchMode`). The parser maps nothing; the caller decides which value is Standard.
    Mode {
        game_mode: u32,
        match_mode: u32,
    },
    // Doc 12 clashes here: the enum tag is "kind", so the field needs another wire name.
    Objective {
        team: u8,
        #[serde(rename = "objective_kind")]
        kind: String,
        t_game: GameSecs,
    },
    SelfState {
        souls: u32,
        cooldowns: Vec<(u32, f32)>,
        flex_slots: u8,
    },
    // Review Mode only. M-06 never forwards these:
    Position {
        account_id: AccountId,
        x: f32,
        y: f32,
        z: f32,
    },
    Damage {
        attacker: Option<AccountId>,
        victim: AccountId,
        amount: u32,
        source: String,
    },
    Death {
        account_id: AccountId,
        t_game: GameSecs,
    },
    Chat {
        account_id: AccountId,
        team_only: bool,
        text: String,
    }, // parsed but never displayed (DEC-025)
    End {
        winning_team: u8,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct RosterEntry {
    pub account_id: AccountId,
    pub name: String,
    pub team: u8,
    pub hero_id: HeroId,
}
