use super::*;
use crate::tactical::{Mode, SessionEvent, View, decode_action, encode_action};
use ferravex_net::{ClientExtension as CX, ServerExtension as SX};
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
impl GameServer {
    pub fn enable_tactical(&mut self, options: TacticalOptions) -> io::Result<()> {
        if !(2..=MAX_PLAYERS).contains(&options.players)
            || !self.peers.is_empty()
            || self.game.players.len() > options.players
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "configure a 2-16 player tactical lobby before peers join",
            ));
        }
        let mut sample = Match::new(self.game.arena.clone());
        sample.spawn_player(0);
        sample.spawn_player(1);
        Mode::new(options.config.clone(), &mut sample).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid tactical configuration",
            )
        })?;
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
        let result =
            self.tactical
                .as_mut()
                .ok_or(RuleError::WrongPhase)?
                .act(&mut self.game, id, action);
        if result.is_ok() {
            self.round_dirty = true;
        }
        result
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
    pub(super) fn prepare_tactical(&mut self) -> io::Result<()> {
        let Some(options) = &self.tactical_options else {
            return Ok(());
        };
        if self.game.players.is_empty() {
            self.tactical = None;
            return Ok(());
        }
        if self.tactical.is_none() && self.game.players.len() == options.players {
            self.tactical = Some(Mode::new(options.config.clone(), &mut self.game).map_err(
                |_| io::Error::new(io::ErrorKind::InvalidInput, "invalid tactical roster"),
            )?);
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
    pub(super) fn round_payload(&mut self) -> io::Result<Option<Vec<u8>>> {
        if let Some(mode) = &self.tactical
            && (self.round_dirty || self.game.tick.is_multiple_of(16))
        {
            let view = mode.view(self.game.tick);
            return SessionEvent::Round(view)
                .encode()
                .map(Some)
                .map_err(protocol_error);
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
            .map_err(protocol_error);
        }
        Ok(None)
    }
    pub(super) fn receive_command(
        &mut self,
        index: usize,
        event: ferravex_net::reliable::EventPacket,
    ) -> io::Result<()> {
        if decode_action(&event.payload).is_err() {
            self.rejected_packets += 1;
            return Ok(());
        }
        let peer = &mut self.peers[index];
        let fresh = event.sequence >= peer.commands.next_sequence()
            && !peer.commands.contains_buffered(event.sequence);
        if fresh
            && peer.events.pending() + peer.commands.buffered()
                >= ferravex_net::reliable::EVENT_LIMIT
        {
            return Ok(());
        }
        let delivered = match peer.commands.receive(&event) {
            Ok(v) => v,
            Err(_) => {
                self.rejected_packets += 1;
                return Ok(());
            }
        };
        peer.last_seen = Instant::now();
        let id = peer.id;
        let session = peer.session;
        let addr = peer.addr;
        for command in delivered {
            let action = decode_action(&command.payload).expect("buffered command validated");
            let result = self.act_local(id, action);
            let bytes = SessionEvent::ActionResult {
                tick: self.game.tick,
                sequence: command.sequence,
                result,
            }
            .encode()
            .map_err(protocol_error)?;
            self.peers[index].events.queue(&bytes).map_err(|e| {
                io::Error::other(format!("command result capacity invariant: {e:?}"))
            })?;
        }
        let ack = encode_server(&ServerPacket::Extension(SX::CommandAck {
            session,
            sequence: event.sequence,
        }))
        .map_err(protocol_error)?;
        send_ok(self.socket.send_to(&ack, addr))
    }
}
impl GameClient {
    pub fn tactical_lobby(&self) -> Option<(u8, u8)> {
        self.lobby
    }
    pub fn tactical_view(&self) -> Option<&View> {
        self.round.as_ref()
    }
    pub fn last_action_result(&self) -> Option<(u64, Result<(), RuleError>)> {
        self.action_result
    }
    pub fn queue_action(&mut self, action: Action) -> io::Result<u64> {
        if self.session.is_none() || self.round.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "tactical session is not ready",
            ));
        }
        let bytes = encode_action(action);
        decode_action(&bytes).map_err(protocol_error)?;
        self.commands
            .queue(&bytes)
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))
    }
    pub fn pending_command_count(&self) -> usize {
        self.commands.pending()
    }
    pub(super) fn flush_commands(&mut self) -> io::Result<()> {
        let Some(session) = self.session else {
            return Ok(());
        };
        for event in self.commands.due(Instant::now().as_micros() / 1000, 8) {
            let bytes = encode_client(&ClientPacket::Extension(CX::Command { session, event }))
                .map_err(protocol_error)?;
            send_ok(self.socket.send(&bytes))?;
        }
        Ok(())
    }
    pub(super) fn consume_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Lobby {
                players, required, ..
            } => {
                self.lobby = Some((players, required));
                self.round = None;
            }
            SessionEvent::Gameplay(e) => self.delivered_events.push_back(e),
            SessionEvent::Round(v) => {
                self.lobby = None;
                self.round = Some(v);
                if let Some(state) = self.players.iter().find(|p| Some(p.id) == self.id) {
                    let mut corrected = state.clone();
                    for input in &self.pending {
                        predict(
                            self.round.as_ref(),
                            &self.arena,
                            &mut corrected,
                            input.input,
                        );
                    }
                    self.predicted = Some(corrected);
                }
            }
            SessionEvent::ActionResult {
                sequence, result, ..
            } => self.action_result = Some((sequence, result)),
        }
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
            .last_seen = Instant::now() - PEER_TIMEOUT - Duration::from_secs(1);
        server.tick().unwrap();
        a.poll().unwrap();
        assert_eq!(a.tactical_view().unwrap().phase, Phase::Finished);
        assert_eq!(a.tactical_view().unwrap().score, [2, 1]);
        assert_eq!(server.peer_count(), 1);
        assert!(server.game.player(id).is_none());
    }
}
