//! Tactical authority adapter; transport carries commands and views, never hits.
use super::*;
use ferravex_play::rounds::{self, Action, Bomb, Phase, RuleError, TacticalMatch, Team, Weapon};
mod wire;
pub use wire::{SessionEvent, decode_action, encode_action};
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerView {
    pub id: u8,
    pub team: Team,
    pub health: u8,
    pub money: u16,
    pub weapon: Weapon,
    pub armor: u8,
    pub kit: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub tick: u32,
    pub phase: Phase,
    pub round: u32,
    pub remaining_ticks: u32,
    pub score: [u16; 2],
    pub bomb: Bomb,
    pub interaction: Option<(u8, u16)>,
    pub players: Vec<PlayerView>,
}
impl View {
    pub fn player(&self, id: u8) -> Option<&PlayerView> {
        self.players.iter().find(|p| p.id == id)
    }
    pub fn weapon(&self, id: u8) -> WeaponSettings {
        weapon_settings(self.player(id).map_or(Weapon::Pistol, |p| p.weapon))
    }
    pub fn filter(&self, id: u8, mut input: InputFrame) -> InputFrame {
        if !matches!(self.phase, Phase::Live | Phase::Planted)
            || self.player(id).is_none_or(|p| p.health == 0)
        {
            input.move_x = 0.;
            input.move_z = 0.;
            input.buttons = 0;
        }
        input
    }
    pub fn predict(&self, player: &mut Player, input: InputFrame, arena: &Arena) {
        if !player.is_alive() {
            return;
        }
        let input = self.filter(player.id, input);
        if !matches!(self.phase, Phase::Live | Phase::Planted) {
            player.velocity = Vec3::ZERO;
            player.yaw = input.yaw;
            player.pitch = input.pitch;
            return;
        }
        step_player_with_settings(player, input, arena, self.weapon(player.id));
    }
}
pub fn weapon_settings(weapon: Weapon) -> WeaponSettings {
    match weapon {
        Weapon::Pistol => WeaponSettings {
            magazine: 12,
            fire_ticks: 12,
            reload_ticks: 96,
        },
        Weapon::Rifle => WeaponSettings::default(),
        Weapon::Sniper => WeaponSettings {
            magazine: 5,
            fire_ticks: 64,
            reload_ticks: 192,
        },
    }
}
pub struct Mode {
    pub rules: TacticalMatch,
}
impl Mode {
    pub fn new(config: rounds::Config, game: &mut Match) -> Result<Self, RuleError> {
        if game.players.len() > MAX_PLAYERS
            || game.context_hash().is_err()
            || game.players.iter().any(|p| p.id as usize >= MAX_PLAYERS)
        {
            return Err(RuleError::InvalidRoster);
        }
        let roster = game
            .players
            .iter()
            .map(|p| rounds::PlayerSetup {
                id: u16::from(p.id),
                team: if p.id % 2 == 0 {
                    Team::Attack
                } else {
                    Team::Defend
                },
                spawn: game.arena.spawn(p.id).to_array(),
            })
            .collect();
        let mode = Self {
            rules: TacticalMatch::new(config, roster)?,
        };
        mode.reset(game);
        Ok(mode)
    }
    fn reset(&self, game: &mut Match) {
        let states: Vec<_> = self
            .rules
            .players()
            .iter()
            .map(|p| {
                (
                    p.id as u8,
                    Vec3::from(p.position),
                    weapon_settings(p.weapon).magazine,
                )
            })
            .collect();
        game.reset_round_players(&states);
    }
    pub fn act(&mut self, game: &mut Match, id: u8, action: Action) -> Result<(), RuleError> {
        self.rules.act(u16::from(id), action)?;
        if matches!(action, Action::BuyWeapon(_))
            && let (Some(p), Some(authority)) =
                (game.player_mut(id), self.rules.player(u16::from(id)))
        {
            p.ammo = weapon_settings(authority.weapon).magazine;
            p.fire_cooldown = 0;
            p.reload_ticks = 0;
        }
        Ok(())
    }
    pub fn step(
        &mut self,
        game: &mut Match,
        inputs: &[(u8, InputFrame)],
        shots: &[ShotTime],
    ) -> RewindStep {
        let round = self.rules.round();
        let mut result = game.step_with_rules(inputs, shots, self);
        for player in &game.players {
            if player.is_alive() {
                let _ = self
                    .rules
                    .set_position(u16::from(player.id), player.position.to_array());
            }
        }
        self.rules.tick(FIXED_DT).expect("fixed finite rule step");
        if self.rules.round() != round {
            self.reset(game);
            result.events.extend(
                game.players
                    .iter()
                    .map(|p| GameEvent::Respawned { player: p.id }),
            );
        }
        let mut absent = [0u16; MAX_PLAYERS];
        let mut count = 0;
        for player in self.rules.players() {
            if game.player(player.id as u8).is_none() {
                absent[count] = player.id;
                count += 1;
            }
        }
        for id in &absent[..count] {
            let _ = self.rules.disconnect(*id);
        }
        for player in &mut game.players {
            if let Some(authority) = self.rules.player(u16::from(player.id)) {
                player.health = authority.health as u8;
                if !player.is_alive() {
                    player.respawn_ticks = 0;
                    player.velocity = Vec3::ZERO;
                }
            }
        }
        result
    }
    pub fn view(&self, tick: u32) -> View {
        View {
            tick,
            phase: self.rules.phase(),
            round: self.rules.round(),
            remaining_ticks: (self.rules.remaining() * TICK_RATE as f32).ceil().max(0.) as u32,
            score: self.rules.score(),
            bomb: self.rules.bomb(),
            interaction: self
                .rules
                .interaction_progress()
                .map(|(id, p)| (id as u8, (p * 65535.).round() as u16)),
            players: self
                .rules
                .players()
                .iter()
                .map(|p| PlayerView {
                    id: p.id as u8,
                    team: p.team,
                    health: p.health as u8,
                    money: p.money as u16,
                    weapon: p.weapon,
                    armor: p.armor as u8,
                    kit: p.defuse_kit,
                })
                .collect(),
        }
    }
}
impl CombatRules for Mode {
    fn frozen(&self, _id: u8) -> bool {
        !matches!(self.rules.phase(), Phase::Live | Phase::Planted)
    }
    fn auto_respawn(&self) -> bool {
        false
    }
    fn weapon(&self, id: u8) -> WeaponSettings {
        weapon_settings(
            self.rules
                .player(u16::from(id))
                .map_or(Weapon::Pistol, |p| p.weapon),
        )
    }
    fn damage(&mut self, attacker: u8, victim: &Player, headshot: bool) -> Option<u8> {
        let weapon = self.rules.player(u16::from(attacker))?.weapon;
        let damage = match (weapon, headshot) {
            (Weapon::Pistol, false) => 26,
            (Weapon::Pistol, true) => 78,
            (Weapon::Rifle, false) => 34,
            (Weapon::Rifle, true) | (Weapon::Sniper, _) => 100,
        };
        let _ = self
            .rules
            .set_position(u16::from(victim.id), victim.position.to_array());
        let before = self.rules.player(u16::from(victim.id))?.health;
        self.rules
            .damage(u16::from(attacker), u16::from(victim.id), damage)
            .ok()?;
        let after = self.rules.player(u16::from(victim.id))?.health;
        Some((before - after) as u8)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(intermission: f32) -> (Match, Mode) {
        let mut g = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -6.)],
        });
        g.spawn_player(0);
        g.spawn_player(1);
        let mode = Mode::new(
            rounds::Config {
                buy_seconds: FIXED_DT,
                round_seconds: 20.,
                plant_seconds: FIXED_DT * 2.,
                defuse_seconds: FIXED_DT * 3.,
                bomb_seconds: 5.,
                intermission_seconds: intermission,
                rounds_to_win: 2,
                starting_money: 4000,
                sites: vec![rounds::Site {
                    center: [0.; 3],
                    radius: 3.,
                }],
                ..Default::default()
            },
            &mut g,
        )
        .unwrap();
        (g, mode)
    }
    #[test]
    fn freeze_economy_armor_weapon_timing_and_no_deathmatch_respawn() {
        let (mut g, mut mode) = setup(10.);
        mode.act(&mut g, 0, Action::BuyWeapon(Weapon::Rifle))
            .unwrap();
        mode.act(&mut g, 1, Action::BuyArmor).unwrap();
        assert_eq!(mode.rules.player(0).unwrap().money, 1300);
        assert_eq!(g.player(0).unwrap().ammo, 30);
        let input = [(
            0,
            InputFrame {
                move_z: 1.,
                buttons: BUTTON_FIRE,
                ..Default::default()
            },
        )];
        let start = g.player(0).unwrap().position;
        assert!(mode.step(&mut g, &input, &[]).events.is_empty());
        assert_eq!(g.player(0).unwrap().position, start);
        assert_eq!(mode.rules.phase(), Phase::Live);
        let shot = [(
            0,
            InputFrame {
                buttons: BUTTON_FIRE,
                ..Default::default()
            },
        )];
        let first = mode.step(&mut g, &shot, &[]).events;
        assert!(first.contains(&GameEvent::Hit {
            attacker: 0,
            victim: 1,
            damage: 50,
            headshot: true
        }));
        assert_eq!(g.player(1).unwrap().health, 50);
        assert_eq!(mode.rules.player(1).unwrap().armor, 50);
        for _ in 0..7 {
            mode.step(&mut g, &shot, &[]);
        }
        assert_eq!(g.player(1).unwrap().health, 0);
        assert_eq!(mode.rules.phase(), Phase::PostRound);
        for _ in 0..250 {
            mode.step(&mut g, &[], &[]);
        }
        assert_eq!(
            g.player(1).unwrap().health,
            0,
            "tactical death never uses practice auto-respawn"
        );
        assert_eq!(mode.rules.score(), [1, 0]);
    }
    #[test]
    fn actual_plant_defuse_round_reset_invalidates_history_and_forfeit_finishes() {
        let (mut g, mut mode) = setup(FIXED_DT * 3.);
        mode.step(&mut g, &[], &[]);
        mode.act(&mut g, 0, Action::Plant(0)).unwrap();
        for _ in 0..2 {
            mode.step(&mut g, &[], &[]);
        }
        assert_eq!(mode.rules.phase(), Phase::Planted);
        g.player_mut(1).unwrap().position = Vec3::X;
        mode.rules.set_position(1, Vec3::X.to_array()).unwrap();
        mode.act(&mut g, 1, Action::Defuse).unwrap();
        for _ in 0..3 {
            mode.step(&mut g, &[], &[]);
        }
        assert_eq!(mode.rules.score(), [0, 1]);
        let old = g.tick;
        for _ in 0..3 {
            mode.step(&mut g, &[], &[]);
        }
        assert_eq!(mode.rules.round(), 2);
        assert_eq!(g.player(1).unwrap().position, g.arena.spawn(1));
        assert_eq!(
            g.rewind_hit_query(0, old, 0., 0.),
            Err(RewindError::HistoryUnavailable)
        );
        mode.rules.forfeit(Team::Attack);
        let v = mode.view(g.tick);
        assert_eq!(v.phase, Phase::Finished);
        assert_eq!(v.score, [0, 2]);
        assert_eq!(v.player(0).unwrap().health, 0);
    }
    #[test]
    fn state_and_command_wire_reject_hostile_rows() {
        let (g, mode) = setup(1.);
        let view = mode.view(g.tick);
        let event = SessionEvent::Round(view.clone());
        let bytes = event.encode().unwrap();
        assert_eq!(SessionEvent::decode(&bytes).unwrap(), event);
        for n in 0..bytes.len() {
            assert!(SessionEvent::decode(&bytes[..n]).is_err());
        }
        let mut invalid = view;
        invalid.players[1].id = invalid.players[0].id;
        assert!(SessionEvent::Round(invalid).encode().is_err());
        for action in [
            Action::BuyWeapon(Weapon::Rifle),
            Action::BuyArmor,
            Action::BuyDefuseKit,
            Action::Plant(1),
            Action::Defuse,
            Action::Cancel,
            Action::DropBomb,
            Action::PickUpBomb,
        ] {
            assert_eq!(decode_action(&encode_action(action)).unwrap(), action);
        }
        assert!(decode_action(&[6, 255]).is_err());
        assert!(decode_action(&[7, 1]).is_err());
        for result in [
            Ok(()),
            Err(RuleError::InsufficientFunds),
            Err(RuleError::WrongPhase),
        ] {
            let e = SessionEvent::ActionResult {
                tick: 1,
                sequence: 4,
                result,
            };
            assert_eq!(SessionEvent::decode(&e.encode().unwrap()).unwrap(), e);
        }
        let mut player = g.players[0].clone();
        let before = player.clone();
        assert!(!step_player_with_settings(
            &mut player,
            InputFrame::default(),
            &g.arena,
            WeaponSettings {
                magazine: 0,
                ..Default::default()
            }
        ));
        assert_eq!(player, before);
    }
}
