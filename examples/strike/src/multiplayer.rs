//! Browser datagram host for the same Rust client/authority as native UDP.
//! Exports belong to Strike alone; other games do not acquire this ABI.
use ferravex_fps::tactical::View;
use ferravex_fps::{Arena, GameClient, GameEvent, GameServer, InputFrame, Player, TacticalOptions};
use ferravex_play::rounds::{Action, Phase, Weapon};
use std::{cell::RefCell, collections::VecDeque};
#[repr(C)]
#[derive(Clone, Copy)]
struct Info {
    mode: u32,
    connected: u32,
    player: u32,
    tick: u32,
    rtt_ms: f32,
    jitter_ms: f32,
    phase: u32,
    round: u32,
    score_a: u32,
    score_d: u32,
    money: u32,
    weapon: u32,
    health: u32,
    ammo: u32,
    x: f32,
    y: f32,
    z: f32,
    pending_commands: u32,
    incoming: u32,
    outgoing: u32,
    peers: u32,
    required: u32,
    bomb_kind: u32,
    carrier: u32,
    bomb_x: f32,
    bomb_y: f32,
    bomb_z: f32,
    interaction: u32,
    progress: f32,
    team: u32,
    remaining_ticks: u32,
    pending_inputs: u32,
    ack_sequence: u32,
    ack_valid: u32,
}
impl Default for Info {
    fn default() -> Self {
        Self {
            mode: 0,
            connected: 0,
            player: u32::MAX,
            tick: 0,
            rtt_ms: -1.,
            jitter_ms: 0.,
            phase: u32::MAX,
            round: 0,
            score_a: 0,
            score_d: 0,
            money: 0,
            weapon: 0,
            health: 0,
            ammo: 0,
            x: 0.,
            y: 0.,
            z: 0.,
            pending_commands: 0,
            incoming: 0,
            outgoing: 0,
            peers: 0,
            required: 0,
            bomb_kind: u32::MAX,
            carrier: u32::MAX,
            bomb_x: 0.,
            bomb_y: 0.,
            bomb_z: 0.,
            interaction: u32::MAX,
            progress: 0.,
            team: u32::MAX,
            remaining_ticks: 0,
            pending_inputs: 0,
            ack_sequence: 0,
            ack_valid: 0,
        }
    }
}
#[derive(Default)]
struct Bridge {
    command: Option<(u32, u64)>,
    actions: VecDeque<Action>,
    edit: Vec<u8>,
    out: Vec<u8>,
    info: Info,
    message: String,
}
thread_local! {static BRIDGE:RefCell<Bridge>=RefCell::new(Bridge::default());}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_start(mode: u32, seed_lo: u32, seed_hi: u32) -> u32 {
    if mode & !0x103 != 0 || mode & 3 > 2 {
        return 0;
    }
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.command = Some((mode, u64::from(seed_lo) | (u64::from(seed_hi) << 32)));
        b.actions.clear();
    });
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_time(micros: f64) -> u32 {
    if !micros.is_finite() || !(0.0..=(1u64 << 53) as f64).contains(&micros) {
        return 0;
    }
    u32::from(ferravex_fps::browser::set_time(micros.floor() as u64))
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_buffer() -> usize {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.edit.resize(1200, 0);
        b.edit.as_mut_ptr() as usize
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_submit(len: usize) -> u32 {
    BRIDGE.with(|b| {
        let b = b.borrow();
        if len == 0 || len > 1200 || len > b.edit.len() {
            return 0;
        }
        u32::from(ferravex_fps::browser::deliver(&b.edit[..len]))
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_next() -> usize {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        if let Some(bytes) = ferravex_fps::browser::take_outgoing() {
            b.out = bytes;
            b.out.len()
        } else {
            0
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_packet() -> usize {
    BRIDGE.with(|b| b.borrow().out.as_ptr() as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_info() -> usize {
    BRIDGE.with(|b| std::ptr::from_ref(&b.borrow().info) as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_info_len() -> usize {
    std::mem::size_of::<Info>()
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_message() -> usize {
    BRIDGE.with(|b| b.borrow().message.as_ptr() as usize)
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_message_len() -> usize {
    BRIDGE.with(|b| b.borrow().message.len())
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_mp_action(opcode: u32, argument: u32) -> u32 {
    if opcode > 255 || argument > 255 {
        return 0;
    }
    let Ok(action) = ferravex_fps::tactical::decode_action(&[opcode as u8, argument as u8]) else {
        return 0;
    };
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        if b.actions.len() == 32 {
            return 0;
        }
        b.actions.push_back(action);
        1
    })
}
#[derive(Default)]
pub struct Session {
    mode: u32,
    server: Option<GameServer>,
    client: Option<GameClient>,
    host_view: Option<View>,
    pub message: String,
    last_result: Option<u64>,
}
impl Session {
    pub fn action_result(&self) -> Option<(u64, Result<(), ferravex_play::rounds::RuleError>)> {
        self.client
            .as_ref()
            .and_then(GameClient::last_action_result)
    }
    pub fn active(&self) -> bool {
        self.mode != 0
    }
    pub fn connected(&self) -> bool {
        self.server.is_some() || self.client.as_ref().is_some_and(GameClient::is_connected)
    }
    pub fn round(&self) -> Option<&View> {
        self.client
            .as_ref()
            .and_then(GameClient::tactical_view)
            .or(self.host_view.as_ref())
    }
    pub fn lobby(&self) -> Option<(u8, u8)> {
        self.client
            .as_ref()
            .and_then(GameClient::tactical_lobby)
            .or_else(|| {
                self.server
                    .as_ref()
                    .and_then(GameServer::tactical_lobby)
                    .map(|(a, b)| (a as u8, b as u8))
            })
    }
    pub fn local(&self) -> Option<&Player> {
        if let Some(server) = &self.server {
            server.game.player(0)
        } else {
            self.client.as_ref().and_then(GameClient::local_player)
        }
    }
    pub fn players(&self) -> Vec<Player> {
        self.server.as_ref().map_or_else(
            || {
                self.client
                    .as_ref()
                    .map_or_else(Vec::new, GameClient::rendered_players)
            },
            |s| s.game.players.clone(),
        )
    }
    pub fn tick_number(&self) -> u32 {
        self.server.as_ref().map_or_else(
            || self.client.as_ref().map_or(0, GameClient::server_tick),
            |s| s.game.tick,
        )
    }
    pub fn before_frame(&mut self) -> bool {
        let command = BRIDGE.with(|b| b.borrow_mut().command.take());
        let changed = command.is_some();
        if let Some((mode, seed)) = command {
            self.server = None;
            self.client = None;
            self.host_view = None;
            self.mode = 0;
            self.last_result = None;
            ferravex_fps::browser::reset(seed);
            let result = match mode & 3 {
                1 => GameServer::with_arena("virtual-authority", Arena::default()).and_then(
                    |mut s| {
                        s.game.spawn_player(0);
                        if mode & 0x100 != 0 {
                            s.enable_tactical(TacticalOptions::default())?;
                        }
                        self.server = Some(s);
                        Ok(())
                    },
                ),
                2 => GameClient::with_arena("virtual-authority", Arena::default())
                    .map(|c| self.client = Some(c)),
                _ => Ok(()),
            };
            match result {
                Ok(()) => {
                    self.mode = mode & 3;
                    self.message = if self.mode == 0 {
                        "Offline practice"
                    } else {
                        "Connecting to authority"
                    }
                    .into();
                }
                Err(e) => self.message = format!("Connection failed: {e}"),
            }
        }
        if let Some(s) = &mut self.server
            && let Err(e) = s.poll()
        {
            self.message = e.to_string();
        }
        if let Some(c) = &mut self.client {
            if let Err(e) = c.poll() {
                self.message = e.to_string();
            }
            if let Some((sequence, result)) = c.last_action_result()
                && self.last_result != Some(sequence)
            {
                self.last_result = Some(sequence);
                self.message = match result {
                    Ok(()) => "Command accepted".into(),
                    Err(e) => format!("Command rejected: {}", crate::hud::rule_feedback(e)),
                };
            }
        }
        let actions = BRIDGE.with(|b| std::mem::take(&mut b.borrow_mut().actions));
        for action in actions {
            self.act(action);
        }
        changed
    }
    pub fn act(&mut self, action: Action) {
        let result = if let Some(server) = &mut self.server {
            server
                .act_local(0, action)
                .map_err(|e| crate::hud::rule_feedback(e).to_owned())
        } else if let Some(client) = &mut self.client {
            client
                .queue_action(action)
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            Err("Tactical session is not ready".into())
        };
        if let Err(e) = result {
            self.message = format!("Command rejected: {e}");
        }
    }
    pub fn tick(&mut self, input: InputFrame) -> Vec<GameEvent> {
        if let Some(server) = &mut self.server {
            match server.tick_with_local(&[(0, input)]) {
                Ok(events) => {
                    self.host_view = server.tactical_view();
                    events
                }
                Err(e) => {
                    self.message = e.to_string();
                    Vec::new()
                }
            }
        } else if let Some(client) = &mut self.client {
            if let Err(e) = client.tick(input) {
                self.message = e.to_string();
            }
            client.drain_events().into_iter().map(|m| m.event).collect()
        } else {
            Vec::new()
        }
    }
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        self.client.as_mut().map_or_else(Vec::new, |c| {
            c.drain_events().into_iter().map(|m| m.event).collect()
        })
    }
    pub fn sync(&self) {
        let mut info = Info {
            mode: self.mode,
            connected: u32::from(self.connected()),
            tick: self.tick_number(),
            ..Info::default()
        };
        if let Some(player) = self.local() {
            info.player = u32::from(player.id);
            info.health = u32::from(player.health);
            info.ammo = u32::from(player.ammo);
            info.x = player.position.x;
            info.y = player.position.y;
            info.z = player.position.z;
        }
        if let Some(client) = &self.client {
            info.rtt_ms = client
                .round_trip_seconds()
                .map_or(-1., |s| (s * 1000.) as f32);
            info.jitter_ms = (client.jitter_seconds() * 1000.) as f32;
            info.pending_commands = client.pending_command_count() as u32;
            info.pending_inputs = client.pending_input_count() as u32;
            if let Some(ack) = client.acknowledged_sequence() {
                info.ack_sequence = ack;
                info.ack_valid = 1;
            }
        }
        if let Some(view) = self.round() {
            info.phase = match view.phase {
                Phase::Buy => 0,
                Phase::Live => 1,
                Phase::Planted => 2,
                Phase::PostRound => 3,
                Phase::Finished => 4,
            };
            info.round = view.round;
            info.remaining_ticks = view.remaining_ticks;
            use ferravex_play::rounds::{Bomb, Team};
            match view.bomb {
                Bomb::Carried(id) => {
                    info.bomb_kind = 0;
                    info.carrier = u32::from(id);
                }
                Bomb::Dropped(p) => {
                    info.bomb_kind = 1;
                    info.bomb_x = p[0];
                    info.bomb_y = p[1];
                    info.bomb_z = p[2];
                }
                Bomb::Planted { position: p, .. } => {
                    info.bomb_kind = 2;
                    info.bomb_x = p[0];
                    info.bomb_y = p[1];
                    info.bomb_z = p[2];
                }
            }
            if let Some((id, progress)) = view.interaction {
                info.interaction = u32::from(id);
                info.progress = f32::from(progress) / 65535.;
            }
            info.score_a = u32::from(view.score[0]);
            info.score_d = u32::from(view.score[1]);
            if let Some(p) = view.player(info.player as u8) {
                info.money = u32::from(p.money);
                info.team = u32::from(p.team == Team::Defend);
                info.weapon = match p.weapon {
                    Weapon::Pistol => 0,
                    Weapon::Rifle => 1,
                    Weapon::Sniper => 2,
                };
            }
        }
        if let Some((a, b)) = self.lobby() {
            info.peers = u32::from(a);
            info.required = u32::from(b);
        }
        let (incoming, outgoing) = ferravex_fps::browser::pending();
        info.incoming = incoming as u32;
        info.outgoing = outgoing as u32;
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            b.info = info;
            if b.message != self.message {
                b.message.clone_from(&self.message);
            }
        });
    }
}
