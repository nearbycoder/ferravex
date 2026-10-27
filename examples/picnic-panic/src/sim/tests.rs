//! Behavioural regression tests for the rules: every prank's completion
//! conditions, perception and pursuit, hiding, carrying, the jointed set
//! pieces, the finale gate and the escape. Inputs are ordinary [`Input`]s;
//! setups only move the raccoon or props as a level designer would.
use super::folk::{ALARMED, CALM, Mode};
use super::park::{PropKind, Role};
use super::pranks::{Bonus, Prank};
use super::props::{Holder, find, hold_pose, place};
use super::raccoon::{Hide, set_down};
use super::*;
use ferravex_render::prelude::vec2;

fn game() -> Game {
    let mut g = Game::new(RunConfig {
        seed: 5,
        completed: 0,
    });
    // Send everyone far away so set-piece tests are not interrupted.
    calm_folk(&mut g);
    g
}

fn calm_folk(g: &mut Game) {
    for (i, f) in g.folk.iter_mut().enumerate() {
        f.pos = vec3(-27. + i as f32 * 1.5, 0., -28.5);
        f.prev = f.pos;
        f.mode = Mode::React(1e6);
    }
}

fn run(g: &mut Game, input: Input, ticks: usize) {
    for _ in 0..ticks {
        g.update(input);
    }
}

fn idle(g: &mut Game, ticks: usize) {
    run(g, Input::default(), ticks);
}

fn face(g: &mut Game, at: Vec3, toward: Vec3) {
    let yaw = yaw_of(flat(toward - at));
    set_down(g, at, yaw);
}

fn press(g: &mut Game, input: Input) {
    g.update(input);
    g.update(Input {
        jump_held: input.jump_held,
        grab_held: input.grab_held,
        ..Input::default()
    });
}

fn grab() -> Input {
    Input {
        grab: true,
        ..Input::default()
    }
}

fn events(g: &mut Game) -> Vec<Event> {
    let mut out = Vec::new();
    g.drain_events(&mut out);
    out
}

#[test]
fn a_fresh_park_settles_and_stays_finite() {
    let mut g = Game::new(RunConfig::default());
    idle(&mut g, 240);
    assert!(g.coon.pos().is_finite());
    assert!(g.props.iter().all(|p| p.pos.is_finite() && p.pos.y > -1.));
    assert_eq!(g.completed(), 0);
}

#[test]
fn raccoon_jumps_from_the_lawn() {
    let mut g = game();
    let start = g.coon.pos().y;
    let mut peak = start;
    for t in 0..60 {
        let press = t == 2;
        g.update(Input {
            jump: press,
            jump_held: press,
            ..Input::default()
        });
        peak = peak.max(g.coon.pos().y);
    }
    assert!(peak > start + 0.9, "jump apex {peak} from {start}");
    assert!(g.coon.motor.grounded);
}

#[test]
fn stealing_the_pie_into_the_den() {
    let mut g = game();
    let pie = find(&g, PropKind::Pie);
    let at = g.props[pie].pos;
    face(&mut g, vec3(at.x, 0., at.z + 0.9), at);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(pie), "{:?}", g.coon.prompt);
    assert_eq!(g.props[pie].holder, Holder::Raccoon);
    // Carry it (the motor walks; here the designer drops us by the den).
    let den = g.park.dens[0];
    face(&mut g, den + vec3(1.2, 0., -1.2), den);
    idle(&mut g, 2);
    assert!(
        g.props[pie].pos.distance(g.coon.carry_anchor(0.05)) < 0.2,
        "pie travels in paws"
    );
    press(&mut g, grab());
    idle(&mut g, 30);
    assert!(g.todo.done(Prank::Pie));
    assert!(g.props[pie].settled, "Nana stops fetching a stashed pie");
}

#[test]
fn swapping_the_marrow_for_a_pinecone_wins_first_prize() {
    let mut g = game();
    let marrow = find(&g, PropKind::Marrow);
    let cone = find(&g, PropKind::Pinecone);
    let plinth = g.park.zone(Role::Plinth).at;
    // A pinecone on the plinth beside the marrow is not a swap yet.
    place(
        &mut g,
        cone,
        hold_pose(plinth + vec3(0.3, 0.15, -0.3), 0.),
        Vec3::ZERO,
    );
    idle(&mut g, 30);
    assert!(!g.todo.done(Prank::Marrow));
    let home = g.props[cone].home;
    place(&mut g, cone, hold_pose(home, 0.), Vec3::ZERO);
    idle(&mut g, 5);
    // Take the marrow away (standing on hind legs at the show table)...
    let m = g.props[marrow].pos;
    face(&mut g, vec3(14., 0., -1.95), m);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(marrow));
    face(&mut g, vec3(10., 0., 1.), vec3(9., 0., 1.));
    press(&mut g, grab());
    idle(&mut g, 20);
    assert!(!g.todo.done(Prank::Marrow), "an empty plinth is not a swap");
    // ...then fetch the pinecone and set it on the plinth from the front.
    let c = g.props[cone].pos;
    face(&mut g, vec3(c.x, 0., c.z + 0.7), c);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(cone));
    face(&mut g, vec3(14., 0., -1.95), plinth);
    idle(&mut g, 1);
    assert!(
        matches!(g.coon.prompt, raccoon::Prompt::Place(_)),
        "{:?}",
        g.coon.prompt
    );
    press(&mut g, grab());
    idle(&mut g, 40);
    assert!(g.todo.done(Prank::Marrow));
    assert!(g.folk[1].award_pending, "Mr. Pumble has a rosette to award");
}

#[test]
fn walking_off_with_the_sheet_music_puts_the_band_off_key() {
    let mut g = game();
    let sheet = find(&g, PropKind::Sheet);
    let stand = g.park.zone(Role::Stand).at;
    face(&mut g, vec3(stand.x, 0.22, stand.z + 0.85), stand);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(sheet), "{:?}", g.coon.prompt);
    let mut offkey = false;
    for _ in 0..180 {
        g.update(Input {
            movement: vec2(0.3, -1.),
            cam_yaw: std::f32::consts::PI,
            ..Input::default()
        });
        offkey |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::OffKey { .. }));
    }
    assert!(g.todo.done(Prank::Music) && g.band_offkey && offkey);
}

#[test]
fn a_loaded_tea_tray_sleds_down_the_hill() {
    let mut g = game();
    let tray = find(&g, PropKind::Tray);
    let blanket = find(&g, PropKind::Blanket);
    // Carry the tray to the crest, set it down, then load the blanket on it.
    let t = g.props[tray].pos;
    face(&mut g, vec3(t.x, 3., t.z + 0.8), t);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(tray));
    face(&mut g, vec3(1., 3.02, -23.4), vec3(1., 3., -21.)); // facing downhill
    press(&mut g, grab());
    idle(&mut g, 30);
    let b = g.props[blanket].pos;
    face(&mut g, vec3(b.x - 0.6, 3.02, b.z), b);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(blanket));
    let t = g.props[tray].pos;
    face(&mut g, t - vec3(0., 0., 0.9) + Vec3::Y * 0.02, t);
    idle(&mut g, 1);
    assert_eq!(g.coon.prompt, raccoon::Prompt::Load);
    press(&mut g, grab());
    assert!(g.props[tray].loaded);
    assert_eq!(g.props[blanket].holder, Holder::Tray(tray));
    // One belly-slide into the tray sends it over the crest.
    let mut sledded = false;
    for k in 0..360 {
        let input = Input {
            movement: vec2(0., 1.),
            cam_yaw: std::f32::consts::PI,
            slide: k % 40 == 10,
            ..Input::default()
        };
        g.update(input);
        sledded |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Sled { .. }));
    }
    assert!(
        sledded && g.todo.done(Prank::Sled),
        "tray ended at {:?}",
        g.props[tray].pos
    );
    assert!(
        g.props[blanket].pos.distance(g.props[tray].pos) < 0.4,
        "the blanket rides along"
    );
}

#[test]
fn holding_the_rope_rings_the_hinged_bell() {
    let mut g = game();
    let rope = g.park.zone(Role::Rope).at;
    face(&mut g, rope, rope + Vec3::Z);
    let before = g.mech.bell_rot;
    g.update(Input {
        grab: true,
        grab_held: true,
        ..Input::default()
    });
    run(
        &mut g,
        Input {
            grab_held: true,
            ..Input::default()
        },
        50,
    );
    assert!(g.todo.done(Prank::Bell));
    idle(&mut g, 20);
    assert!(
        g.mech.bell_rot.angle_between(before) > 0.2,
        "the bell swings on its hinge"
    );
    // Releasing early does not ring: the hold restarts from zero.
    let mut g = game();
    face(&mut g, rope, rope + Vec3::Z);
    run(
        &mut g,
        Input {
            grab_held: true,
            ..Input::default()
        },
        20,
    );
    idle(&mut g, 2);
    run(
        &mut g,
        Input {
            grab_held: true,
            ..Input::default()
        },
        20,
    );
    assert!(!g.todo.done(Prank::Bell));
}

#[test]
fn a_whoopee_cushion_only_counts_when_someone_sits_on_it() {
    let mut g = game();
    let cushion = find(&g, PropKind::Cushion);
    let seat = g.park.seat(0).unwrap().at;
    place(
        &mut g,
        cushion,
        hold_pose(seat + Vec3::Y * 0.08, 0.),
        Vec3::ZERO,
    );
    idle(&mut g, 30);
    assert!(!g.todo.done(Prank::Cushion), "an empty bench is not funny");
    // Nana walks back to her bench and sits.
    let nana = &mut g.folk[0];
    nana.mode = Mode::Routine;
    nana.spot = 0;
    nana.arrived = false;
    nana.pos = vec3(-9.2, 0., 7.6);
    let mut pfft = false;
    for _ in 0..600 {
        g.update(Input::default());
        pfft |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Pfft { folk: 0, .. }));
        if pfft {
            break;
        }
    }
    assert!(pfft && g.todo.done(Prank::Cushion));
}

#[test]
fn a_stomp_from_the_hay_bales_launches_the_melon() {
    let mut g = game();
    let melon = find(&g, PropKind::Melon);
    let pivot = g.mech.seesaw_pivot;
    // Melon on the low (-X) end, raccoon on the high bale.
    place(
        &mut g,
        melon,
        hold_pose(pivot + vec3(-1.9, 0.2, 0.), 0.),
        Vec3::ZERO,
    );
    idle(&mut g, 60);
    face(&mut g, vec3(16.0, 1.25, 19.), vec3(10., 1.2, 19.));
    idle(&mut g, 10);
    assert!(
        g.coon.pos().y > 1.1,
        "standing on the bale at {:?}",
        g.coon.pos()
    );
    let mut launched = false;
    for k in 0..150 {
        let input = Input {
            movement: vec2(-0.5, 0.),
            cam_yaw: 0.,
            jump: k == 2,
            jump_held: k < 6,
            ..Input::default()
        };
        g.update(input);
        launched |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Launch { .. }));
    }
    assert!(launched && g.todo.done(Prank::Melon));
}

#[test]
fn the_finale_waits_for_the_list_then_the_escape_wins() {
    let mut g = game();
    let cloth = g.park.zone(Role::Cloth).at;
    face(&mut g, cloth, cloth + Vec3::X);
    run(
        &mut g,
        Input {
            grab: true,
            grab_held: true,
            ..Input::default()
        },
        90,
    );
    assert!(
        !g.todo.done(Prank::Finale) && !g.coon.cape,
        "locked until seven pranks"
    );
    for p in &Prank::ALL[..7] {
        pranks::complete(&mut g, *p, Vec3::ZERO);
    }
    assert!(g.todo.finale_open());
    calm_folk(&mut g);
    face(&mut g, cloth, cloth + Vec3::X);
    g.update(Input {
        grab: true,
        grab_held: true,
        ..Input::default()
    });
    run(
        &mut g,
        Input {
            grab_held: true,
            ..Input::default()
        },
        60,
    );
    assert!(g.todo.done(Prank::Finale) && g.coon.cape);
    assert!(
        g.folk.iter().all(|f| f.mode == Mode::Chase),
        "everyone gives chase"
    );
    calm_folk(&mut g);
    let den = g.park.dens[0];
    face(&mut g, den + vec3(1., 0., -1.), den);
    idle(&mut g, 4);
    assert!(g.is_over() && g.todo.done(Prank::Escape));
}

#[test]
fn picnickers_spot_chase_catch_and_carry_the_raccoon_home() {
    let mut g = game();
    // Mr. Pumble stands still at his show table, looking south.
    let p = &mut g.folk[1];
    p.mode = Mode::Routine;
    p.pos = vec3(14., 0., -1.7);
    p.yaw = yaw_of(Vec3::Z);
    p.look_yaw = p.yaw;
    p.arrived = true;
    p.wait = 100.;
    face(&mut g, vec3(14., 0., 2.5), vec3(14., 0., 10.));
    let mut spotted = false;
    let mut caught = false;
    for _ in 0..600 {
        g.update(Input::default());
        for e in events(&mut g) {
            spotted |= matches!(e, Event::Spotted { folk: 1, .. });
            caught |= matches!(e, Event::Caught { folk: 1, .. });
        }
        if caught {
            break;
        }
    }
    assert!(spotted && caught, "state {:?}", g.folk[1].mode);
    assert_eq!(g.coon.scruffed, Some(1));
    assert_eq!(g.stats.scruffs, 1);
    // Carried to the burrow and shooed.
    let mut shooed = false;
    for _ in 0..60 * 40 {
        g.update(Input::default());
        shooed |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Shooed { .. }));
        if shooed {
            break;
        }
    }
    assert!(shooed && g.coon.scruffed.is_none());
    assert!(
        flat(g.coon.pos() - g.park.dens[0]).length()
            < flat(vec3(14., 0., 2.5) - g.park.dens[0]).length() - 10.,
        "carried a good way toward the burrow"
    );
    assert!(g.coon.grace > 0., "a moment of dignity after being shooed");
}

#[test]
fn wriggling_six_times_escapes_a_carry() {
    let mut g = game();
    let p = &mut g.folk[2];
    p.mode = Mode::Routine;
    p.pos = vec3(0., 0., 5.);
    p.yaw = yaw_of(Vec3::Z);
    p.look_yaw = p.yaw;
    p.arrived = true;
    p.wait = 100.;
    face(&mut g, vec3(0., 0., 7.), vec3(0., 0., 12.));
    for _ in 0..600 {
        g.update(Input::default());
        if g.coon.scruffed.is_some() {
            break;
        }
    }
    assert_eq!(g.coon.scruffed, Some(2));
    for _ in 0..6 {
        press(
            &mut g,
            Input {
                jump: true,
                ..Input::default()
            },
        );
    }
    assert!(g.coon.scruffed.is_none());
    assert!(
        events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::WriggledFree { folk: 2, .. }))
    );
    assert!(matches!(g.folk[2].mode, Mode::React(_)));
}

#[test]
fn hiding_in_a_bush_breaks_line_of_sight_and_searchers_give_up() {
    let mut g = game();
    let bush = g.park.bushes[3].at;
    let p = &mut g.folk[0];
    p.mode = Mode::Routine;
    p.pos = bush + vec3(0., 0., -5.);
    p.yaw = yaw_of(Vec3::Z);
    p.look_yaw = p.yaw;
    p.arrived = true;
    p.wait = 100.;
    set_down(&mut g, bush, 0.);
    idle(&mut g, 3);
    assert!(matches!(g.coon.hide, Hide::Bush(3)));
    run(&mut g, Input::default(), 120);
    assert!(!g.folk[0].sees, "a bush is just a bush");
    assert_eq!(g.folk[0].state(), CALM);
    // Step out and she notices; dive back in and she searches, then gives up.
    set_down(&mut g, bush + vec3(0., 0., -2.), 0.);
    let mut alarmed = false;
    for _ in 0..120 {
        g.update(Input::default());
        alarmed |= g.folk[0].state() == ALARMED;
        if alarmed {
            break;
        }
    }
    assert!(alarmed);
    set_down(&mut g, bush + vec3(0.3, 0., 0.3), 0.);
    let mut gave_up = false;
    for _ in 0..60 * 30 {
        g.update(Input::default());
        gave_up |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::GaveUp { folk: 0, .. }));
        if g.coon.scruffed.is_some() || gave_up {
            break;
        }
    }
    assert!(
        gave_up || g.coon.scruffed.is_some(),
        "searchers either find you or give up"
    );
}

#[test]
fn chittering_lures_a_listener_to_the_noise() {
    let mut g = game();
    let p = &mut g.folk[3];
    p.mode = Mode::Routine;
    p.pos = vec3(0., 0., 10.);
    p.yaw = yaw_of(Vec3::NEG_Z);
    p.look_yaw = p.yaw;
    p.arrived = true;
    p.wait = 100.;
    // Behind a hedge-free spot, out of sight but in earshot.
    set_down(&mut g, vec3(0., 0., 16.), 0.);
    press(
        &mut g,
        Input {
            chitter: true,
            ..Input::default()
        },
    );
    let mut curious = false;
    for _ in 0..90 {
        g.update(Input::default());
        curious |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Suspicious { folk: 3, .. }));
    }
    assert!(curious, "Pip heard something");
    assert_eq!(g.stats.chitters, 1);
}

#[test]
fn throwing_an_apple_sends_it_flying_and_drops_restore_collisions() {
    let mut g = game();
    let apple = g
        .props
        .iter()
        .position(|p| p.kind == PropKind::Apple)
        .unwrap();
    let at = g.props[apple].pos;
    face(&mut g, vec3(at.x, 0., at.z - 0.8), at);
    press(&mut g, grab());
    assert_eq!(g.coon.held, Some(apple));
    let start = g.props[apple].pos;
    press(
        &mut g,
        Input {
            throw: true,
            ..Input::default()
        },
    );
    assert!(g.coon.held.is_none());
    assert!(g.props[apple].vel.length() > 3.);
    idle(&mut g, 120);
    assert!(flat(g.props[apple].pos - start).length() > 2.5, "it flew");
    assert!(
        g.props[apple].pos.y > -0.2,
        "and landed on the lawn, colliding again"
    );
}

#[test]
fn bonuses_and_pond_dunks() {
    let mut g = game();
    let apple = g
        .props
        .iter()
        .position(|p| p.kind == PropKind::Apple)
        .unwrap();
    let pond = g.park.zone(Role::Pond).at;
    place(
        &mut g,
        apple,
        hold_pose(pond + Vec3::Y * 0.3, 0.),
        Vec3::ZERO,
    );
    idle(&mut g, 30);
    assert!(g.todo.bonus & Bonus::Splash.bit() != 0);
    assert!(
        g.props[apple].pos.distance(g.props[apple].home) < 0.5,
        "the pond returns things"
    );
}

#[test]
fn owners_fetch_their_belongings_back_home() {
    let mut g = game();
    let pie = find(&g, PropKind::Pie);
    place(&mut g, pie, hold_pose(vec3(-5., 0.1, 9.), 0.), Vec3::ZERO);
    let nana = &mut g.folk[0];
    nana.mode = Mode::Routine;
    nana.pos = vec3(-7., 0., 7.25);
    nana.spot = 1;
    nana.arrived = true;
    nana.wait = 100.;
    nana.yaw = yaw_of(vec3(2., 0., 1.8));
    nana.look_yaw = nana.yaw;
    let mut returned = false;
    for _ in 0..60 * 30 {
        g.update(Input::default());
        returned |= events(&mut g)
            .iter()
            .any(|e| matches!(e, Event::Returned { folk: 0, prop, .. } if *prop == pie));
        if returned {
            break;
        }
    }
    assert!(returned);
    assert!(g.props[pie].pos.distance(g.props[pie].home) < 0.3);
}

/// Tuning aid: `cargo test -p picnic-panic bot_trace -- --ignored --nocapture`
/// prints the autoplay raccoon's full run.
#[test]
#[ignore]
fn bot_trace() {
    let mut game = Game::new(RunConfig {
        seed: 3,
        completed: 0,
    });
    let mut bot = bot::Bot::new(&game);
    let mut events = Vec::new();
    let mut last_task = bot.task;
    for _ in 0..60 * 60 * 14 {
        let input = bot.input(&game);
        game.update(input);
        game.drain_events(&mut events);
        for e in events.drain(..) {
            match e {
                Event::Prank { prank, .. } => println!("{:7.1}s PRANK {prank:?}", game.time),
                Event::Bonus { bonus, .. } => println!("{:7.1}s bonus {bonus:?}", game.time),
                Event::Caught { folk, .. } => println!("{:7.1}s caught by {folk}", game.time),
                Event::Victory { .. } => println!("{:7.1}s VICTORY", game.time),
                _ => {}
            }
        }
        if bot.task != last_task {
            println!("{:7.1}s task {:?} -> {:?}", game.time, last_task, bot.task);
            last_task = bot.task;
        }
        if game.is_over() {
            break;
        }
    }
    println!(
        "done: {:07b} in {:.0}s, scruffs {}",
        game.completed(),
        game.time,
        game.stats.scruffs
    );
}

#[test]
fn the_demo_raccoon_completes_several_pranks_with_ordinary_inputs() {
    let mut game = Game::new(RunConfig {
        seed: 3,
        completed: 0,
    });
    let mut bot = bot::Bot::new(&game);
    for _ in 0..60 * 60 * 6 {
        let input = bot.input(&game);
        game.update(input);
        game.drain_events(&mut Vec::new());
        if game.todo.count() >= 5 {
            break;
        }
    }
    assert!(
        game.todo.count() >= 5,
        "bot finished {:07b}",
        game.completed()
    );
}

/// Tuning aid: start with the pranks in binary `MASK` already done and trace
/// the bot's next task.
#[test]
#[ignore]
fn debug_bot_task() {
    let mask: u8 = std::env::var("MASK")
        .ok()
        .and_then(|v| u8::from_str_radix(&v, 2).ok())
        .unwrap_or(0b1110111);
    let mut game = Game::new(RunConfig {
        seed: 3,
        completed: mask,
    });
    let mut bot = bot::Bot::new(&game);
    for tick in 0..60 * 120 {
        let input = bot.input(&game);
        game.update(input);
        let mut ev = Vec::new();
        game.drain_events(&mut ev);
        for e in &ev {
            match e {
                Event::Prank { .. }
                | Event::Caught { .. }
                | Event::Grabbed { .. }
                | Event::Dropped { .. }
                | Event::Stomp { .. }
                | Event::Launch { .. } => println!("{:6.1} {e:?}", game.time),
                _ => {}
            }
        }
        if tick % 60 == 0 {
            let t = &game.props[props::find(&game, park::PropKind::Tray)];
            let m = &game.props[props::find(&game, park::PropKind::Melon)];
            let _ = (t, m);
            println!(
                "{:6.1} task {:?} coon {:?} held {:?} speed {:.1} nana {:?} {:?} state {}",
                game.time,
                bot.task,
                game.coon.pos(),
                game.coon.held,
                game.coon.speed,
                game.folk[0].pos,
                game.folk[0].mode,
                game.folk[0].state()
            );
        }
        if game.todo.count() == 7 {
            println!("all done at {:.1}", game.time);
            break;
        }
    }
}
