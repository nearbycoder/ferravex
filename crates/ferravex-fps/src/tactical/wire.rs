use super::*;
use crate::checkpoint::{Reader, Writer};
use ferravex_net::ProtocolError;
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    Lobby {
        tick: u32,
        players: u8,
        required: u8,
    },
    Gameplay(NetworkEvent),
    Round(View),
    ActionResult {
        tick: u32,
        sequence: u64,
        result: Result<(), RuleError>,
    },
}
pub fn encode_action(action: Action) -> [u8; 2] {
    match action {
        Action::BuyWeapon(Weapon::Pistol) => [1, 0],
        Action::BuyWeapon(Weapon::Rifle) => [2, 0],
        Action::BuyWeapon(Weapon::Sniper) => [3, 0],
        Action::BuyArmor => [4, 0],
        Action::BuyDefuseKit => [5, 0],
        Action::Plant(site) => [6, u8::try_from(site).unwrap_or(255)],
        Action::Defuse => [7, 0],
        Action::Cancel => [8, 0],
        Action::DropBomb => [9, 0],
        Action::PickUpBomb => [10, 0],
    }
}
pub fn decode_action(b: &[u8]) -> Result<Action, ProtocolError> {
    if b.len() != 2 || b[0] != 6 && b[1] != 0 {
        return Err(ProtocolError::Invalid);
    }
    Ok(match b[0] {
        1 => Action::BuyWeapon(Weapon::Pistol),
        2 => Action::BuyWeapon(Weapon::Rifle),
        3 => Action::BuyWeapon(Weapon::Sniper),
        4 => Action::BuyArmor,
        5 => Action::BuyDefuseKit,
        6 if b[1] < 32 => Action::Plant(b[1] as usize),
        7 => Action::Defuse,
        8 => Action::Cancel,
        9 => Action::DropBomb,
        10 => Action::PickUpBomb,
        _ => return Err(ProtocolError::Invalid),
    })
}
fn error_code(e: RuleError) -> u8 {
    match e {
        RuleError::InvalidConfig => 1,
        RuleError::InvalidRoster => 2,
        RuleError::UnknownPlayer => 3,
        RuleError::WrongPhase => 4,
        RuleError::Dead => 5,
        RuleError::WrongTeam => 6,
        RuleError::InsufficientFunds => 7,
        RuleError::AlreadyOwned => 8,
        RuleError::BombUnavailable => 9,
        RuleError::OutOfRange => 10,
        RuleError::Busy => 11,
        RuleError::InvalidInput => 12,
    }
}
fn read_error(v: u8) -> Result<Result<(), RuleError>, ProtocolError> {
    Ok(match v {
        0 => Ok(()),
        1 => Err(RuleError::InvalidConfig),
        2 => Err(RuleError::InvalidRoster),
        3 => Err(RuleError::UnknownPlayer),
        4 => Err(RuleError::WrongPhase),
        5 => Err(RuleError::Dead),
        6 => Err(RuleError::WrongTeam),
        7 => Err(RuleError::InsufficientFunds),
        8 => Err(RuleError::AlreadyOwned),
        9 => Err(RuleError::BombUnavailable),
        10 => Err(RuleError::OutOfRange),
        11 => Err(RuleError::Busy),
        12 => Err(RuleError::InvalidInput),
        _ => return Err(ProtocolError::Invalid),
    })
}
fn phase(v: Phase) -> u8 {
    match v {
        Phase::Buy => 0,
        Phase::Live => 1,
        Phase::Planted => 2,
        Phase::PostRound => 3,
        Phase::Finished => 4,
    }
}
fn valid(v: &View) -> bool {
    v.round > 0
        && v.round <= 1_000_000
        && v.remaining_ticks <= 3600 * TICK_RATE
        && v.players.len() >= 2
        && v.players.len() <= MAX_PLAYERS
        && v.players.iter().enumerate().all(|(i, p)| {
            (p.id as usize) < MAX_PLAYERS
                && v.players[..i].iter().all(|old| old.id != p.id)
                && p.health <= 100
                && p.money <= 16000
                && p.armor <= 100
        })
        && v.interaction.is_none_or(|(id, _)| v.player(id).is_some())
        && match v.bomb {
            Bomb::Carried(id) => {
                id < MAX_PLAYERS as u16
                    && v.player(id as u8).is_some_and(|p| p.team == Team::Attack)
            }
            Bomb::Dropped(pos) => valid_pos(pos),
            Bomb::Planted { site, position } => site < 32 && valid_pos(position),
        }
}
fn valid_pos(p: [f32; 3]) -> bool {
    p.into_iter().all(|n| n.is_finite() && n.abs() <= 8192.)
}
impl SessionEvent {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        if let Self::Gameplay(e) = self {
            return e.encode();
        }
        let mut out = Vec::with_capacity(192);
        let mut w = Writer(&mut |b| out.extend_from_slice(b));
        match self {
            Self::Lobby {
                tick,
                players,
                required,
            } => {
                if !(2..=MAX_PLAYERS as u8).contains(required) || players > required {
                    return Err(ProtocolError::Invalid);
                }
                w.u32(*tick);
                w.u8(0x82);
                w.u8(1);
                w.u8(*players);
                w.u8(*required);
            }
            Self::Gameplay(_) => unreachable!(),
            Self::ActionResult {
                tick,
                sequence,
                result,
            } => {
                w.u32(*tick);
                w.u8(0x81);
                w.u8(1);
                w.u64(*sequence);
                w.u8(result.err().map_or(0, error_code));
            }
            Self::Round(v) => {
                if !valid(v) {
                    return Err(ProtocolError::Invalid);
                }
                w.u32(v.tick);
                w.u8(0x80);
                w.u8(1);
                w.u8(phase(v.phase));
                w.u32(v.round);
                w.u32(v.remaining_ticks);
                for n in v.score {
                    w.u16(n);
                }
                let (tag, id, site, pos) = match v.bomb {
                    Bomb::Carried(id) => (0, id as u8, 0, [0.; 3]),
                    Bomb::Dropped(pos) => (1, 0, 0, pos),
                    Bomb::Planted { site, position } => (2, 0, site as u8, position),
                };
                w.u8(tag);
                w.u8(id);
                w.u8(site);
                for n in pos {
                    w.f32(n);
                }
                w.u8(v.interaction.is_some() as u8);
                let (id, progress) = v.interaction.unwrap_or((0, 0));
                w.u8(id);
                w.u16(progress);
                w.u8(v.players.len() as u8);
                for p in &v.players {
                    w.u8(p.id);
                    w.u8((p.team == Team::Defend) as u8);
                    w.u8(p.health);
                    w.u16(p.money);
                    w.u8(match p.weapon {
                        Weapon::Pistol => 0,
                        Weapon::Rifle => 1,
                        Weapon::Sniper => 2,
                    });
                    w.u8(p.armor);
                    w.u8(p.kit as u8);
                }
            }
        }
        Ok(out)
    }
    pub fn decode(b: &[u8]) -> Result<Self, ProtocolError> {
        if b.len() < 6 || b.len() > 900 {
            return Err(ProtocolError::Invalid);
        }
        if b[4] < 0x80 {
            return Ok(Self::Gameplay(NetworkEvent::decode(b)?));
        }
        if b[5] != 1 {
            return Err(ProtocolError::Version);
        }
        let invalid = ProtocolError::Invalid;
        let mut r = Reader { b, i: 0 };
        let tick = r.u32().map_err(|_| invalid)?;
        r.i = 6;
        let result = match b[4] {
            0x82 => {
                let players = r.u8().map_err(|_| invalid)?;
                let required = r.u8().map_err(|_| invalid)?;
                if !(2..=MAX_PLAYERS as u8).contains(&required) || players > required {
                    return Err(invalid);
                }
                Self::Lobby {
                    tick,
                    players,
                    required,
                }
            }
            0x81 => {
                let sequence = r.u64().map_err(|_| invalid)?;
                let result = read_error(r.u8().map_err(|_| invalid)?)?;
                Self::ActionResult {
                    tick,
                    sequence,
                    result,
                }
            }
            0x80 => {
                let phase = match r.u8().map_err(|_| invalid)? {
                    0 => Phase::Buy,
                    1 => Phase::Live,
                    2 => Phase::Planted,
                    3 => Phase::PostRound,
                    4 => Phase::Finished,
                    _ => return Err(invalid),
                };
                let round = r.u32().map_err(|_| invalid)?;
                let remaining_ticks = r.u32().map_err(|_| invalid)?;
                let score = [r.u16().map_err(|_| invalid)?, r.u16().map_err(|_| invalid)?];
                let tag = r.u8().map_err(|_| invalid)?;
                let id = r.u8().map_err(|_| invalid)?;
                let site = r.u8().map_err(|_| invalid)?;
                let position = r.vec3().map_err(|_| invalid)?.to_array();
                let bomb = match tag {
                    0 if site == 0 && position == [0.; 3] => Bomb::Carried(u16::from(id)),
                    1 if id == 0 && site == 0 => Bomb::Dropped(position),
                    2 if id == 0 => Bomb::Planted {
                        site: site as usize,
                        position,
                    },
                    _ => return Err(invalid),
                };
                let present = r.bool().map_err(|_| invalid)?;
                let id = r.u8().map_err(|_| invalid)?;
                let progress = r.u16().map_err(|_| invalid)?;
                if !present && (id != 0 || progress != 0) {
                    return Err(invalid);
                }
                let interaction = present.then_some((id, progress));
                let count = r.u8().map_err(|_| invalid)? as usize;
                if !(2..=MAX_PLAYERS).contains(&count) {
                    return Err(invalid);
                }
                let mut players = Vec::with_capacity(count);
                for _ in 0..count {
                    players.push(PlayerView {
                        id: r.u8().map_err(|_| invalid)?,
                        team: if r.bool().map_err(|_| invalid)? {
                            Team::Defend
                        } else {
                            Team::Attack
                        },
                        health: r.u8().map_err(|_| invalid)?,
                        money: r.u16().map_err(|_| invalid)?,
                        weapon: match r.u8().map_err(|_| invalid)? {
                            0 => Weapon::Pistol,
                            1 => Weapon::Rifle,
                            2 => Weapon::Sniper,
                            _ => return Err(invalid),
                        },
                        armor: r.u8().map_err(|_| invalid)?,
                        kit: r.bool().map_err(|_| invalid)?,
                    });
                }
                let v = View {
                    tick,
                    phase,
                    round,
                    remaining_ticks,
                    score,
                    bomb,
                    interaction,
                    players,
                };
                if !valid(&v) {
                    return Err(invalid);
                }
                Self::Round(v)
            }
            _ => return Err(invalid),
        };
        if r.i != b.len() {
            return Err(ProtocolError::Trailing);
        }
        Ok(result)
    }
}
