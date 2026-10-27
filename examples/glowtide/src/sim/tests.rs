use super::*;

#[allow(dead_code)]
fn play(config: RunConfig, max_ticks: u64, log: impl FnMut(&Game)) -> Game {
    play_skill(config, max_ticks, false, log)
}

/// `clumsy` approximates a casual human: no dashes, slow reactions.
fn play_skill(config: RunConfig, max_ticks: u64, clumsy: bool, mut log: impl FnMut(&Game)) -> Game {
    let mut game = Game::new(config);
    let mut events = Vec::with_capacity(MAX_EVENTS);
    let mut held = Input::default();
    for tick in 0..max_ticks {
        let mut input = bot::autoplay(&game);
        if clumsy && game.phase == Phase::Running {
            if tick % 9 == 0 {
                held = input;
            }
            input = Input {
                dash: false,
                ..held
            };
        }
        game.update(input);
        game.drain_events(&mut events);
        events.clear();
        log(&game);
        if game.is_over() {
            break;
        }
    }
    game
}

#[test]
#[ignore = "balance report; run with --ignored --nocapture"]
fn tune_report() {
    for (quick, seed, clumsy) in [
        (false, 1, false),
        (false, 2, false),
        (false, 6, false),
        (false, 7, false),
        (true, 3, false),
        (true, 8, false),
        (false, 4, true),
        (false, 9, true),
        (true, 5, true),
    ] {
        let config = RunConfig {
            quick,
            seed,
            ..RunConfig::default()
        };
        let mut last_minute = -1;
        let mut peak = 0;
        let game = play_skill(config, 60 * 60 * 14, clumsy, |g| {
            peak = peak.max(g.foes.len());
            let m = (g.time / 30.) as i32;
            if m != last_minute && g.phase == Phase::Running {
                last_minute = m;
                let weapons: Vec<String> = g
                    .weapons
                    .iter()
                    .flatten()
                    .map(|w| {
                        format!(
                            "{}{}{}",
                            &w.id.info().name[..4],
                            w.level,
                            if w.evolved { "*" } else { "" }
                        )
                    })
                    .collect();
                println!(
                    "t={:>5.0}s min={:.1} lvl={:>2} hp={:>4.0}/{:<4.0} foes={:>3} motes={:>3} kills={:>5} shots={:>3} {:?}",
                    g.time,
                    g.minute(),
                    g.level(),
                    g.player.hp,
                    g.player.max_hp,
                    g.foes.len(),
                    g.motes.len(),
                    g.stats.kills,
                    g.shots.len(),
                    weapons
                );
            }
        });
        println!(
            "== quick={quick} seed={seed} clumsy={clumsy}: {:?} at {:.0}s lvl {} kills {} peak foes {} dmg {:?} taken {:.0} boss_left {:?}\n",
            game.phase,
            game.time,
            game.level(),
            game.stats.kills,
            peak,
            game.stats.damage.map(|d| d as u64),
            game.stats.damage_taken,
            game.stats.gulpjaw_left,
        );
    }
}

fn quiet_game() -> Game {
    Game::new(RunConfig {
        seed: 99,
        ..RunConfig::default()
    })
}

fn tick(game: &mut Game, input: Input) -> Vec<Event> {
    game.update(input);
    let mut out = Vec::with_capacity(MAX_EVENTS);
    game.drain_events(&mut out);
    out
}

#[test]
fn level_up_offers_distinct_cards_and_reroll_banish_pick_are_atomic() {
    let mut game = quiet_game();
    game.rerolls = 30;
    game.banishes = 1;
    game.enter_level_up();
    assert_eq!(game.phase, Phase::LevelUp);
    let cards = game.offer.cards().to_vec();
    assert!(cards.len() >= 3);
    for (i, a) in cards.iter().enumerate() {
        assert!(!cards[i + 1..].contains(a), "cards are distinct");
    }
    let serial = game.offer.serial;
    tick(
        &mut game,
        Input {
            choice: Choice::Reroll,
            ..Input::default()
        },
    );
    assert_eq!(game.rerolls, 29);
    assert_ne!(game.offer.serial, serial);
    // Banish the first card: it never shows up again.
    let banished = game.offer.cards[0];
    tick(
        &mut game,
        Input {
            choice: Choice::Banish(0),
            ..Input::default()
        },
    );
    assert_eq!(game.banishes, 0);
    for _ in 0..28 {
        assert!(!game.offer.cards().contains(&banished));
        tick(
            &mut game,
            Input {
                choice: Choice::Reroll,
                ..Input::default()
            },
        );
    }
    // Out of banishes: a banish request changes nothing.
    let before = game.offer;
    tick(
        &mut game,
        Input {
            choice: Choice::Banish(1),
            ..Input::default()
        },
    );
    assert_eq!(game.offer, before);
    // Time is frozen while choosing; a pick applies and resumes.
    let time = game.time;
    tick(&mut game, Input::default());
    assert_eq!(game.time, time);
    let card = game.offer.cards[1];
    let level = game.card_next_level(card);
    let events = tick(
        &mut game,
        Input {
            choice: Choice::Pick(1),
            ..Input::default()
        },
    );
    assert!(events.contains(&Event::Picked { card }));
    assert_eq!(game.phase, Phase::Running);
    if let Card::Weapon(_) | Card::Passive(_) = card {
        assert_eq!(game.card_next_level(card), level + 1);
    }
}

#[test]
fn max_weapon_with_its_paired_passive_evolves_from_a_chest() {
    let mut game = quiet_game();
    for _ in 0..6 {
        game.apply_card(Card::Weapon(WeaponId::WispBolt));
    }
    assert_eq!(game.weapon(WeaponId::WispBolt).unwrap().level, WEAPON_MAX);
    assert_eq!(game.evolution_ready(), None, "needs Moon Pearl first");
    game.apply_card(Card::Passive(PassiveId::MoonPearl));
    assert!(game.mods.haste < 1.);
    assert_eq!(game.evolution_ready(), Some(WeaponId::WispBolt));
    game.open_chest(1);
    assert_eq!(game.phase, Phase::Chest);
    assert_eq!(
        game.chest.items()[0],
        ChestItem::Evolved(WeaponId::WispBolt)
    );
    assert!(game.weapon(WeaponId::WispBolt).unwrap().evolved);
    assert!(game.stats.coins > 0, "treasure always includes glimmer");
    tick(
        &mut game,
        Input {
            choice: Choice::Pick(0),
            ..Input::default()
        },
    );
    assert_eq!(game.phase, Phase::Running);
    // Lunar Lance fires piercing lances at a nearby foe.
    let at = game.player.pos + vec2(120., 0.);
    game.spawn_foe(FoeKind::Crabbo, at, 50., 0);
    for _ in 0..90 {
        tick(&mut game, Input::default());
    }
    assert!(game.shots.iter().any(|s| s.kind == ShotKind::Lance) || game.stats.damage[0] > 0.);
    assert!(weapon_stats(WeaponId::WispBolt, 7, true).pierce >= 99);
}

#[test]
fn chests_without_evolutions_level_owned_items_only() {
    let mut game = quiet_game();
    game.open_chest(2);
    assert_eq!(game.chest.items().len(), 3);
    for item in game.chest.items() {
        match item {
            ChestItem::Level(Card::Weapon(w), _) => assert_eq!(*w, WeaponId::WispBolt),
            ChestItem::Level(card, _) => panic!("chest granted unowned {card:?}"),
            _ => {}
        }
    }
    assert!(game.weapon(WeaponId::WispBolt).unwrap().level > 1);
}

#[test]
fn spawn_curve_grows_and_scheduled_beats_arrive_on_the_logical_clock() {
    let mut last = 0.;
    for m in 0..=10 {
        let target = Game::target_population(m as f32);
        assert!(target >= last);
        last = target;
    }
    assert!(Game::target_population(7.) >= 300.);
    // Quick tides run the director clock at double speed.
    let mut quick = Game::new(RunConfig {
        quick: true,
        ..RunConfig::default()
    });
    quick.time = 150.;
    assert!((quick.minute() - 5.).abs() < 1e-4);
    let mut game = quiet_game();
    game.player.invuln = f32::INFINITY;
    game.time = 4.95 * 60.;
    let events = tick(&mut game, Input::default());
    assert!(events.iter().any(|e| matches!(
        e,
        Event::BossSpawn {
            boss: BossKind::MotherBrine
        }
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TideShift { tide: 2 }))
    );
    assert!(events.iter().any(|e| matches!(e, Event::EliteSpawn { .. })));
    assert!(game.boss().is_some_and(|b| b.kind == FoeKind::MotherBrine));
    // Population fills toward the curve over a few seconds.
    for _ in 0..240 {
        tick(&mut game, Input::default());
    }
    assert!(game.foes.len() > 60);
}

#[test]
fn damage_records_per_weapon_knocks_back_and_kills_drop_motes() {
    let mut game = quiet_game();
    game.spawn_foe(FoeKind::Grumpfish, vec2(300., 0.), 1., 0);
    let hp = game.foes[0].hp;
    let killed = game.damage_foe(0, 10., WeaponId::TideLash, vec2(1., 0.), 100.);
    assert!(!killed);
    assert!(game.foes[0].hp < hp);
    assert!(game.foes[0].knock.x > 0.);
    assert!(game.stats.damage[WeaponId::TideLash as usize] >= 10.);
    let motes = game.motes.len();
    assert!(game.damage_foe(0, 1e6, WeaponId::TideLash, Vec2::ZERO, 0.));
    // Overkill is not counted: recorded damage equals the foe's health.
    assert!((game.stats.damage[WeaponId::TideLash as usize] - f64::from(hp)).abs() < 1e-3);
    assert_eq!(game.stats.kills_by[WeaponId::TideLash as usize], 1);
    assert_eq!(game.motes.len(), motes + 1);
    assert!(game.events.iter().any(|e| matches!(
        e,
        Event::Kill {
            kind: FoeKind::Grumpfish,
            ..
        }
    )));
    // Dead foes are removed at the end of the tick.
    tick(&mut game, Input::default());
    assert!(game.foes.iter().all(|f| !f.dead));
}

#[test]
fn motes_vacuum_into_xp_and_level_ups_queue_choices() {
    let mut game = quiet_game();
    game.drop_mote(game.player.pos + vec2(40., 0.), 3);
    let mut collected = false;
    for _ in 0..60 {
        let events = tick(&mut game, Input::default());
        collected |= events
            .iter()
            .any(|e| matches!(e, Event::Collect { value: 3, .. }));
    }
    assert!(collected);
    assert_eq!(game.progression.xp(), 3);
    // Far motes stay put.
    game.drop_mote(game.player.pos + vec2(800., 0.), 1);
    tick(&mut game, Input::default());
    assert!(!game.motes.last().unwrap().pulled);
    // Enough XP for two levels: one modal now, one queued.
    for k in 0..10 {
        game.drop_mote(game.player.pos + vec2(10. + k as f32, 0.), 4);
    }
    let mut leveled = 0;
    for _ in 0..30 {
        if game.phase == Phase::LevelUp {
            leveled += 1;
            tick(
                &mut game,
                Input {
                    choice: Choice::Pick(0),
                    ..Input::default()
                },
            );
        } else {
            tick(&mut game, Input::default());
        }
    }
    assert!(leveled >= 2, "queued level-ups each get their own choice");
    assert!(game.level() >= 3);
}

#[test]
fn boss_phases_escalate_and_gulpjaw_defeat_wins_the_run() {
    let mut game = quiet_game();
    game.player.invuln = f32::INFINITY;
    game.spawn_boss(BossKind::Gulpjaw);
    let i = game
        .foes
        .iter()
        .position(|f| f.kind == FoeKind::Gulpjaw)
        .unwrap();
    let max = game.foes[i].max_hp;
    game.foes[i].hp = max * 0.55;
    let events = tick(&mut game, Input::default());
    assert!(events.contains(&Event::BossPhase {
        boss: BossKind::Gulpjaw,
        phase: 1
    }));
    let i = game
        .foes
        .iter()
        .position(|f| f.kind == FoeKind::Gulpjaw)
        .unwrap();
    game.foes[i].hp = max * 0.2;
    let events = tick(&mut game, Input::default());
    assert!(events.contains(&Event::BossPhase {
        boss: BossKind::Gulpjaw,
        phase: 2
    }));
    // Patterns cycle and telegraph charges before they happen.
    let mut lane = false;
    for _ in 0..600 {
        tick(&mut game, Input::default());
        lane |= game
            .telegraphs
            .iter()
            .any(|t| matches!(t.shape, Shape::Lane { .. }));
    }
    assert!(lane, "charges are telegraphed");
    let i = game
        .foes
        .iter()
        .position(|f| f.kind == FoeKind::Gulpjaw)
        .unwrap();
    game.damage_foe(i, 1e9, WeaponId::WispBolt, Vec2::ZERO, 0.);
    assert_eq!(game.phase, Phase::Won);
    assert!(game.stats.gulpjaw_down);
    assert!(game.events.contains(&Event::Victory));
    assert!(game.bullets.is_empty() && game.telegraphs.is_empty());
}

#[test]
fn slam_telegraphs_hurt_only_inside_and_dashes_grant_iframes() {
    let mut game = quiet_game();
    let p = game.player.pos;
    game.telegraphs.push(Telegraph {
        shape: Shape::Circle {
            pos: p + vec2(500., 0.),
            radius: 60.,
        },
        age: 0.,
        delay: 0.01,
        damage: 30.,
        style: 0,
    });
    tick(&mut game, Input::default());
    assert_eq!(game.player.hp, game.player.max_hp);
    game.telegraphs.push(Telegraph {
        shape: Shape::Circle {
            pos: p,
            radius: 60.,
        },
        age: 0.,
        delay: 0.01,
        damage: 30.,
        style: 0,
    });
    tick(&mut game, Input::default());
    assert!(game.player.hp < game.player.max_hp);
    // A dash edge starts exactly one dash with invulnerability.
    let mut game = quiet_game();
    let events = tick(
        &mut game,
        Input {
            dash: true,
            movement: vec2(1., 0.),
            ..Input::default()
        },
    );
    assert!(events.iter().any(|e| matches!(e, Event::Dash { .. })));
    assert!(game.player.invuln > 0.);
    let hp = game.player.hp;
    assert!(!game.hurt_player(50.));
    assert_eq!(game.player.hp, hp);
    tick(
        &mut game,
        Input {
            dash: true,
            ..Input::default()
        },
    );
    assert_eq!(game.stats.dashes, 1, "cooldown blocks a second dash");
}

#[test]
fn second_wind_revives_once_then_the_run_is_lost() {
    let mut game = Game::new(RunConfig {
        bonuses: Bonuses {
            revives: 1,
            ..Bonuses::default()
        },
        ..RunConfig::default()
    });
    game.player.hp = 0.;
    let events = tick(&mut game, Input::default());
    assert!(events.iter().any(|e| matches!(e, Event::Revive { .. })));
    assert_eq!(game.phase, Phase::Running);
    assert!(game.player.hp > 0.);
    game.player.hp = 0.;
    let events = tick(&mut game, Input::default());
    assert!(events.contains(&Event::Defeat));
    assert_eq!(game.phase, Phase::Lost);
    assert!(game.is_over());
}

#[test]
fn bot_runs_are_deterministic_for_a_seed() {
    let run = |seed| {
        let mut g = Game::new(RunConfig {
            seed,
            ..RunConfig::default()
        });
        for _ in 0..60 * 75 {
            let input = bot::autoplay(&g);
            g.update(input);
            g.events.clear();
        }
        (
            g.player.pos,
            g.stats.kills,
            g.level(),
            g.foes.len(),
            g.total_damage(),
        )
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5).1, 0);
}
