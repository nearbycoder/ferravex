//! Browser transport-independent host/guest state. WebRTC supplies DTLS, this supplies authority.
use ferravex_engine::net::{
    self, ClientPacket, InputFrame, InputQueue, ServerPacket, Snapshot, sequence_newer,
};
use ferravex_fps::{Match, Player, step_player};
use std::collections::VecDeque;
#[derive(Default)]
pub struct Peer {
    pub acknowledged: bool,
    pub mode: u32,
    pub queue: InputQueue,
    pub last: InputFrame,
    pub pending: VecDeque<InputFrame>,
    pub sequence: u32,
    pub received_tick: Option<u32>,
    pub elapsed: f32,
}
impl Peer {
    pub fn status_label(&self) -> &'static str {
        match self.mode {
            1 if !self.acknowledged => "Host: waiting for guest input",
            1 if self.elapsed >= 2. => "Host: guest input stale",
            1 => "Host: guest active",
            2 if self.received_tick.is_none() => "Guest: waiting for host",
            2 if self.elapsed >= 2. => "Guest: host snapshot stale",
            2 => "Guest: predicting; host decides hits",
            _ => "Offline training",
        }
    }
    pub fn set_mode(&mut self, mode: u32) {
        *self = Self {
            mode: mode.min(2),
            ..Default::default()
        };
    }
    pub fn command(&mut self, mut input: InputFrame) -> InputFrame {
        input.sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        if self.mode == 2 {
            if self.pending.len() == 256 {
                self.pending.pop_front();
            }
            self.pending.push_back(input);
        }
        input
    }
    pub fn receive(&mut self, bytes: &[u8], game: &mut Match) -> bool {
        if self.mode == 1 {
            if let Ok(ClientPacket::Input { session: 1, inputs }) = net::decode_client(bytes) {
                let mut accepted = false;
                for input in inputs {
                    accepted |= self.queue.push(input);
                }
                if accepted {
                    self.elapsed = 0.;
                }
                return accepted;
            }
        } else if self.mode == 2
            && let Ok(ServerPacket::Snapshot(snapshot)) = net::decode_server(bytes)
        {
            if snapshot.session != 1
                || self
                    .received_tick
                    .is_some_and(|tick| !sequence_newer(snapshot.tick, tick))
            {
                return false;
            }
            let Some(local) = snapshot.players.iter().find(|p| p.id == 1) else {
                return false;
            };
            let mut predicted = Player::from_snapshot(local);
            predicted.id = 0;
            if snapshot.ack_valid {
                self.pending
                    .retain(|input| sequence_newer(input.sequence, snapshot.ack_sequence));
            }
            for input in &self.pending {
                step_player(&mut predicted, *input, &game.arena);
            }
            let mut players = Vec::with_capacity(snapshot.players.len());
            players.push(predicted);
            for source in snapshot.players.iter().filter(|p| p.id != 1) {
                let mut player = Player::from_snapshot(source);
                if player.id == 0 {
                    player.id = 1;
                }
                players.push(player);
            }
            game.players = players;
            self.received_tick = Some(snapshot.tick);
            self.elapsed = 0.;
            return true;
        }
        false
    }
    pub fn packet(&self, game: &Match) -> Vec<u8> {
        if self.mode == 1 {
            net::encode_server(&ServerPacket::Snapshot(Snapshot {
                epochs: game.life_epochs(),
                session: 1,
                tick: game.tick,
                ack_sequence: self.last.sequence,
                ack_valid: self.acknowledged,
                players: game.players.iter().map(Player::snapshot).collect(),
            }))
            .unwrap()
        } else if self.mode == 2 {
            let mut inputs: Vec<_> = self
                .pending
                .iter()
                .rev()
                .take(net::MAX_INPUTS_PER_PACKET)
                .copied()
                .collect();
            inputs.reverse();
            if inputs.is_empty() {
                return Vec::new();
            }
            net::encode_client(&ClientPacket::Input { session: 1, inputs }).unwrap()
        } else {
            Vec::new()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_fps::Arena;
    #[test]
    fn peer_freshness_requires_new_accepted_input_or_snapshot() {
        let mut host = Match::default();
        host.spawn_player(0);
        host.spawn_player(1);
        let mut guest = Match::default();
        guest.spawn_player(0);
        let mut server = Peer::default();
        server.set_mode(1);
        let mut client = Peer::default();
        client.set_mode(2);
        assert_eq!(server.status_label(), "Host: waiting for guest input");
        assert_eq!(client.status_label(), "Guest: waiting for host");
        client.command(InputFrame::default());
        let command = client.packet(&guest);
        assert!(server.receive(&command, &mut host));
        server.last = server.queue.pop_latest().unwrap();
        server.acknowledged = true;
        assert_eq!(server.status_label(), "Host: guest active");
        server.elapsed = 2.;
        assert!(!server.receive(&command, &mut host));
        assert_eq!(server.elapsed, 2.);
        assert_eq!(server.status_label(), "Host: guest input stale");
        let snapshot = server.packet(&host);
        assert!(client.receive(&snapshot, &mut guest));
        assert_eq!(client.elapsed, 0.);
        client.elapsed = 2.;
        assert!(!client.receive(&snapshot, &mut guest));
        assert_eq!(client.status_label(), "Guest: host snapshot stale");
        host.step(&[]);
        assert!(client.receive(&server.packet(&host), &mut guest));
        assert_eq!(client.elapsed, 0.);
        assert_eq!(
            client.status_label(),
            "Guest: predicting; host decides hits"
        );
    }
    #[test]
    fn short_action_survives_the_snapshot_send_interval() {
        let mut client = Peer::default();
        client.set_mode(2);
        client.command(InputFrame {
            buttons: net::BUTTON_FIRE,
            ..Default::default()
        });
        for _ in 0..4 {
            client.command(InputFrame::default());
        }
        let game = Match::default();
        let bytes = client.packet(&game);
        let ClientPacket::Input { inputs, .. } = net::decode_client(&bytes).unwrap() else {
            panic!("input packet")
        };
        assert!(
            inputs
                .iter()
                .any(|input| input.buttons & net::BUTTON_FIRE != 0)
        );
    }
    #[test]
    fn authority_reconciles_guest_after_dropped_input_and_stale_snapshot_is_rejected() {
        let mut host = Match::new(Arena::default());
        host.spawn_player(0);
        host.spawn_player(1);
        let mut guest = Match::new(Arena::default());
        guest.spawn_player(0);
        let mut server = Peer::default();
        server.set_mode(1);
        let mut client = Peer::default();
        client.set_mode(2);
        for tick in 0..256 {
            let command = client.command(InputFrame {
                move_x: 0.5,
                ..Default::default()
            });
            let arena = guest.arena.clone();
            step_player(guest.player_mut(0).unwrap(), command, &arena);
            if tick % 4 != 0 {
                assert!(server.receive(&client.packet(&guest), &mut host));
            }
            if let Some(input) = server.queue.pop_latest() {
                server.last = input;
                server.acknowledged = true;
            }
            host.step(&[(1, server.last)]);
            if tick % 3 != 0 {
                let packet = server.packet(&host);
                assert!(client.receive(&packet, &mut guest));
                assert!(!client.receive(&packet, &mut guest));
            }
        }
        let packet = server.packet(&host);
        if client.received_tick != Some(host.tick) {
            client.receive(&packet, &mut guest);
        }
        let position = host.player(1).unwrap().position;
        assert!((guest.player(0).unwrap().position - position).length() < 0.3);
        assert!(client.pending.len() < 4);
        assert!(!server.receive(&[0; 1201], &mut host));
    }
}
