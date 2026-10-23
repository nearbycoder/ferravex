use super::server_state::{DriverError, Output, Peer, ServerState};
use super::*;
use crate::tactical::{Mode, SessionEvent, View, decode_action};
#[cfg(test)]
use ferravex_net::ClientExtension as CX;
use ferravex_net::ServerExtension as SX;
use ferravex_play::rounds::{self, Action, Phase, RuleError, Team};
#[derive(Clone, Debug)]
pub struct TacticalOptions {
    pub players: usize,
    pub config: rounds::Config,
}
impl Default for TacticalOptions {
    fn default() -> Self {
        Self {
            players: 2,
            config: Default::default(),
        }
    }
}
struct Lobby;
impl super::super::CombatRules for Lobby {
    fn frozen(&self, _: u8) -> bool {
        true
    }
    fn auto_respawn(&self) -> bool {
        false
    }
}
impl ServerState {
    pub fn enable_tactical(&mut self, options: TacticalOptions) -> Result<(), DriverError> {
        if !(2..=MAX_PLAYERS).contains(&options.players)
            || !self.peers.is_empty()
            || self.game.players.len() > options.players
        {
            return Err(DriverError::TacticalConfiguration);
        }
        let mut sample = Match::new(self.game.arena.clone());
        sample.spawn_player(0);
        sample.spawn_player(1);
        Mode::new(options.config.clone(), &mut sample)
            .map_err(|_| DriverError::TacticalConfiguration)?;
        self.tactical_options = Some(options);
        self.tactical = None;
        self.round_dirty = true;
        Ok(())
    }
    pub fn tactical_view(&self) -> Option<View> {
        self.tactical.as_ref().map(|m| m.view(self.game.tick))
    }
    pub fn tactical_lobby(&self) -> Option<(usize, usize)> {
        self.tactical_options
            .as_ref()
            .map(|o| (self.game.players.len(), o.players))
    }
    pub fn act_local(&mut self, id: u8, action: Action) -> Result<(), RuleError> {
        apply_action(
            &mut self.tactical,
            &mut self.game,
            &mut self.round_dirty,
            id,
            action,
        )
    }
    /// An authority/operator restart keeps the connection and reliable sequence.
    pub fn restart_tactical(&mut self) {
        if self.tactical_options.is_some() {
            self.tactical = None;
            self.round_dirty = true;
            for p in &mut self.peers {
                p.inputs.clear_pending();
                p.last_input.buttons = 0;
                p.last_input.move_x = 0.;
                p.last_input.move_z = 0.;
                p.last_shot = None;
            }
        }
    }
    pub(super) fn may_join(&self) -> bool {
        self.tactical_options.as_ref().is_none_or(|o| {
            self.game.players.len() < o.players
                && (self.tactical.is_none()
                    || self
                        .tactical
                        .as_ref()
                        .is_some_and(|m| m.rules.phase() == Phase::Finished))
        })
    }
    pub(super) fn prepare_tactical(&mut self) -> Result<(), DriverError> {
        let Some(options) = &self.tactical_options else {
            return Ok(());
        };
        if self.game.players.is_empty() {
            self.tactical = None;
            return Ok(());
        }
        if self.tactical.is_none() && self.game.players.len() == options.players {
            self.tactical = Some(
                Mode::new(options.config.clone(), &mut self.game)
                    .map_err(|_| DriverError::TacticalConfiguration)?,
            );
            self.round_dirty = true;
        }
        if let Some(mode) = &mut self.tactical {
            let absent: Vec<_> = mode
                .rules
                .players()
                .iter()
                .filter(|p| p.health > 0 && self.game.player(p.id as u8).is_none())
                .map(|p| p.id)
                .collect();
            for id in absent {
                let _ = mode.rules.disconnect(id);
                self.round_dirty = true;
            }
            for team in [Team::Attack, Team::Defend] {
                if mode.rules.phase() == Phase::Finished {
                    break;
                }
                if !mode
                    .rules
                    .players()
                    .iter()
                    .any(|p| p.team == team && self.game.player(p.id as u8).is_some())
                {
                    mode.rules.forfeit(team);
                    self.round_dirty = true;
                }
            }
        }
        Ok(())
    }
    pub(super) fn step_combat(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shots: &[super::super::ShotTime],
    ) -> super::super::RewindStep {
        if let Some(mode) = &mut self.tactical {
            let phase = mode.rules.phase();
            let round = mode.rules.round();
            let interaction = mode.rules.interaction_progress().map(|v| v.0);
            let result = mode.step(&mut self.game, inputs, shots);
            self.round_dirty |= phase != mode.rules.phase()
                || interaction != mode.rules.interaction_progress().map(|v| v.0)
                || result
                    .events
                    .iter()
                    .any(|e| matches!(e, super::super::GameEvent::Hit { .. }));
            while mode.rules.pop_event().is_some() {
                self.round_dirty = true;
            }
            if round != mode.rules.round() {
                for peer in &mut self.peers {
                    peer.inputs.clear_pending();
                    peer.last_input.buttons = 0;
                    peer.last_input.move_x = 0.;
                    peer.last_input.move_z = 0.;
                    peer.last_shot = None;
                }
            }
            result
        } else if self.tactical_options.is_some() {
            self.game.step_with_rules(inputs, shots, &mut Lobby)
        } else {
            self.game.step_with_rewind(inputs, shots)
        }
    }
    pub(super) fn round_payload(&mut self) -> Result<Option<Vec<u8>>, DriverError> {
        if let Some(mode) = &self.tactical
            && (self.round_dirty || self.game.tick.is_multiple_of(16))
        {
            let view = mode.view(self.game.tick);
            return SessionEvent::Round(view)
                .encode()
                .map(Some)
                .map_err(DriverError::Protocol);
        }
        if let Some(options) = &self.tactical_options
            && self.tactical.is_none()
            && (self.round_dirty || self.game.tick.is_multiple_of(16))
        {
            return SessionEvent::Lobby {
                tick: self.game.tick,
                players: self.game.players.len() as u8,
                required: options.players as u8,
            }
            .encode()
            .map(Some)
            .map_err(DriverError::Protocol);
        }
        Ok(None)
    }
}
// The same application helper is used by operator and durable remote commands.
fn apply_action(
    tactical: &mut Option<Mode>,
    game: &mut Match,
    dirty: &mut bool,
    id: u8,
    action: Action,
) -> Result<(), RuleError> {
    let result = tactical
        .as_mut()
        .ok_or(RuleError::WrongPhase)?
        .act(game, id, action);
    if result.is_ok() {
        *dirty = true;
    }
    result
}
impl ServerState {
    pub(super) fn receive_command(
        &mut self,
        index: usize,
        event: ferravex_net::reliable::EventPacket,
        now: Duration,
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        if decode_action(&event.payload).is_err() {
            output.report.commands_refused += 1;
            output.report.packets_rejected += 1;
            return Ok(());
        }
        let peer = &mut self.peers[index];
        // Full sequence-window admission leaves room for the missing head even
        // if all 255 successors precede it. Results have their own bounded cap.
        match peer
            .commands
            .admit(&event, ferravex_net::reliable::EVENT_LIMIT)
        {
            Ok(ferravex_net::reliable::Receipt::Retained) => output.report.commands_retained += 1,
            Ok(ferravex_net::reliable::Receipt::Duplicate) => output.report.commands_duplicate += 1,
            Err(ferravex_net::reliable::Error::Full) => {
                output.report.commands_refused += 1;
                return Ok(());
            }
            Err(_) => {
                output.report.commands_refused += 1;
                output.report.packets_rejected += 1;
                return Ok(());
            }
        }
        peer.last_seen = now;
        // ACK durable receipt, independently from action execution/result sends.
        output.packet(
            peer.addr,
            &ServerPacket::Extension(SX::CommandAck {
                session: peer.session,
                sequence: event.sequence,
            }),
        )?;
        self.drain_peer_commands(index, output)
    }
    fn flush_result(peer: &mut Peer) -> Result<(), DriverError> {
        if let Some(result) = &peer.pending_result {
            let bytes = result.encode().map_err(DriverError::Protocol)?;
            // Failed queueing preserves the durable result and stops further
            // actions. Sequence exhaustion is a deliberate explicit failure.
            peer.events.queue(&bytes).map_err(DriverError::Reliable)?;
            peer.pending_result = None;
        }
        Ok(())
    }
    fn drain_peer_commands(
        &mut self,
        index: usize,
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        let peer = &mut self.peers[index];
        Self::flush_result(peer)?;
        for _ in 0..EVENT_SEND_BUDGET {
            if peer.events.pending() == ferravex_net::reliable::EVENT_LIMIT {
                break;
            }
            // The result slot and event-window space are checked BEFORE an
            // action. The retained, fixed-size result commits before consuming
            // the ACKed command; no speculative apply-then-refuse rollback.
            let report = peer.commands.drain_ready(
                1,
                ferravex_net::reliable::EVENT_BYTES,
                |sequence, bytes| {
                    let action = decode_action(bytes).expect("validated before durable admission");
                    let result = apply_action(
                        &mut self.tactical,
                        &mut self.game,
                        &mut self.round_dirty,
                        peer.id,
                        action,
                    );
                    peer.pending_result = Some(SessionEvent::ActionResult {
                        tick: self.game.tick,
                        sequence,
                        result,
                    });
                    true
                },
            );
            output.report.commands_executed += report.delivered;
            if report.delivered == 0 {
                break;
            }
            Self::flush_result(peer)?;
        }
        Ok(())
    }
    pub(super) fn drain_commands(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        for index in 0..self.peers.len() {
            self.drain_peer_commands(index, output)?;
        }
        Ok(())
    }
}
pub(super) fn predict(round: Option<&View>, arena: &Arena, player: &mut Player, input: InputFrame) {
    if let Some(view) = round {
        view.predict(player, input, arena);
    } else {
        step_player(player, input, arena);
    }
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use ferravex_math::Vec3;
    use ferravex_play::rounds::{Site, Weapon};
    #[test]
    fn udp_round_commands_reorder_once_then_plant_defuse_reset_and_disconnect_forfeit() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::X],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        server
            .enable_tactical(TacticalOptions {
                players: 2,
                config: rounds::Config {
                    buy_seconds: 2. * super::super::super::FIXED_DT,
                    round_seconds: 10.,
                    bomb_seconds: 1.,
                    plant_seconds: 2. * super::super::super::FIXED_DT,
                    defuse_seconds: 3. * super::super::super::FIXED_DT,
                    intermission_seconds: 3. * super::super::super::FIXED_DT,
                    rounds_to_win: 2,
                    starting_money: 4000,
                    sites: vec![Site {
                        center: [0.; 3],
                        radius: 3.,
                    }],
                    ..Default::default()
                },
            })
            .unwrap();
        let mut a = GameClient::with_arena(server.local_addr().unwrap(), arena.clone()).unwrap();
        let mut b = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        connect_test_clients(&mut server, &mut [&mut a, &mut b]);
        server.tick().unwrap();
        a.poll().unwrap();
        b.poll().unwrap();
        assert_eq!(a.tactical_view().unwrap().phase, Phase::Buy);
        a.queue_action(Action::BuyArmor).unwrap();
        a.queue_action(Action::BuyWeapon(Weapon::Rifle)).unwrap();
        let commands = a.commands.due(Instant::now().as_micros() / 1000, 8);
        assert_eq!(commands.len(), 2);
        for i in [1, 1, 0] {
            let bytes = encode_client(&ClientPacket::Extension(CX::Command {
                session: a.session.unwrap(),
                event: commands[i].clone(),
            }))
            .unwrap();
            a.socket.send(&bytes).unwrap();
        }
        server.tick().unwrap();
        a.poll().unwrap();
        b.poll().unwrap();
        server.poll().unwrap();
        assert_eq!(a.pending_command_count(), 0);
        assert_eq!(server.peers[0].commands.next_sequence(), 2);
        assert_eq!(
            server
                .tactical
                .as_ref()
                .unwrap()
                .rules
                .player(0)
                .unwrap()
                .money,
            650
        );
        assert_eq!(a.tactical_view().unwrap().player(0).unwrap().money, 650);
        assert_eq!(a.last_action_result(), Some((1, Ok(()))));
        assert_eq!(a.tactical_view().unwrap().phase, Phase::Live);
        a.queue_action(Action::Plant(0)).unwrap();
        a.poll().unwrap();
        for _ in 0..2 {
            server.tick().unwrap();
            a.poll().unwrap();
            b.poll().unwrap();
        }
        assert_eq!(a.tactical_view().unwrap().phase, Phase::Planted);
        b.queue_action(Action::Defuse).unwrap();
        b.poll().unwrap();
        for _ in 0..3 {
            server.tick().unwrap();
            a.poll().unwrap();
            b.poll().unwrap();
        }
        assert_eq!(a.tactical_view().unwrap().score, [0, 1]);
        assert_eq!(a.tactical_view().unwrap().phase, Phase::PostRound);
        for _ in 0..3 {
            server.tick().unwrap();
            a.poll().unwrap();
            b.poll().unwrap();
        }
        assert_eq!(a.tactical_view().unwrap().round, 2);
        assert_eq!(server.game.player(0).unwrap().ammo, 30);
        assert_eq!(server.game.player(1).unwrap().ammo, 12);
        server.poll().unwrap();
        let id = b.player_id().unwrap();
        server
            .peers
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .last_seen = Duration::ZERO;
        server.started = server.started - PEER_TIMEOUT - Duration::from_secs(1);
        let now = server.started.elapsed();
        for p in &mut server.state.peers {
            if p.id != id {
                p.last_seen = now;
            }
        }
        server.tick().unwrap();
        a.poll().unwrap();
        assert_eq!(a.tactical_view().unwrap().phase, Phase::Finished);
        assert_eq!(a.tactical_view().unwrap().score, [2, 1]);
        assert_eq!(server.peer_count(), 1);
        assert!(server.game.player(id).is_none());
    }
}
