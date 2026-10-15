//! A host-side integration of the standalone systems; no graphics or transport.
use ferravex_play::{
    animation::{Interpolation, Keyframe, LoopMode, Vec3Track},
    navigation::{Cell, Connectivity, Grid},
    particles::{ParticleDesc, ParticlePool},
    rounds::{Action, Config, Phase, PlayerSetup, Site, TacticalMatch, Team},
    scene::{Scene, Transform, transform_point},
    ui::{Anchors, Rect, UiState, Widget},
};

#[test]
fn navigated_plant_and_ui_defuse_update_scene_animation_and_particles() {
    let mut game = TacticalMatch::new(
        Config {
            buy_seconds: 1.0,
            round_seconds: 60.0,
            bomb_seconds: 20.0,
            plant_seconds: 1.0,
            kit_defuse_seconds: 1.0,
            intermission_seconds: 1.0,
            rounds_to_win: 1,
            starting_money: 1000,
            sites: vec![Site {
                center: [4.0, 0.0, 4.0],
                radius: 1.0,
            }],
            ..Config::default()
        },
        vec![
            PlayerSetup {
                id: 1,
                team: Team::Attack,
                spawn: [0.0; 3],
            },
            PlayerSetup {
                id: 2,
                team: Team::Defend,
                spawn: [4.0, 0.0, 4.0],
            },
        ],
    )
    .unwrap();
    game.act(2, Action::BuyDefuseKit).unwrap();
    game.tick(1.0).unwrap();
    let mut grid = Grid::new(5, 5).unwrap();
    grid.set_blocked(Cell { x: 2, y: 2 }, true).unwrap();
    let route = grid
        .path(
            Cell { x: 0, y: 0 },
            Cell { x: 4, y: 4 },
            Connectivity::Eight,
        )
        .unwrap();
    let track = Vec3Track::new(
        route
            .iter()
            .enumerate()
            .map(|(index, cell)| Keyframe {
                time: index as f32,
                value: [cell.x as f32, 0.0, cell.y as f32],
            })
            .collect(),
        Interpolation::Linear,
    )
    .unwrap();
    let mut scene = Scene::new();
    let actor = scene
        .create("attacker", Transform::default(), None)
        .unwrap();
    for frame in 0..=track.duration() as u32 * 60 {
        let position = track.sample(frame as f32 / 60.0, LoopMode::Clamp).unwrap();
        game.set_position(1, position).unwrap();
        scene
            .set_local(
                actor,
                Transform {
                    translation: position,
                    ..Transform::default()
                },
            )
            .unwrap();
        game.tick(1.0 / 60.0).unwrap();
    }
    assert_eq!(
        transform_point(scene.world_matrix(actor).unwrap(), [0.0; 3]),
        [4.0, 0.0, 4.0]
    );
    game.act(1, Action::Plant(0)).unwrap();
    game.tick(1.0).unwrap();
    assert_eq!(game.phase(), Phase::Planted);
    let mut sparks = ParticlePool::new(8);
    sparks
        .spawn(ParticleDesc {
            position: [4.0, 0.0, 4.0],
            velocity: [0.0, 2.0, 0.0],
            ..ParticleDesc::default()
        })
        .unwrap();
    sparks.update(0.5).unwrap();
    assert_eq!(sparks.iter().next().unwrap().position, [4.0, 1.0, 4.0]);
    let button = Anchors {
        min: [0.5; 2],
        max: [0.5; 2],
        offset_min: [-40.0, -20.0],
        offset_max: [40.0, 20.0],
    }
    .resolve(Rect {
        min: [0.0; 2],
        size: [640.0, 480.0],
    })
    .unwrap();
    let mut ui = UiState::new();
    ui.add(Widget {
        id: 7,
        rect: button,
        clip: None,
        enabled: true,
        focusable: true,
    })
    .unwrap();
    ui.end_frame();
    ui.pointer_down([320.0, 240.0]);
    if ui.pointer_up([320.0, 240.0]) == Some(7) {
        game.act(2, Action::Defuse).unwrap();
    }
    game.tick(2.0).unwrap();
    assert_eq!(game.phase(), Phase::Finished);
    assert_eq!(game.score(), [0, 1]);
    let encoded = scene.encode().unwrap();
    assert_eq!(Scene::decode(&encoded).unwrap().encode().unwrap(), encoded);
}
