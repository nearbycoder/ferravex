//! Tin Automata: twelve robot-programming workbenches and a factory to restore.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity, controls and the complete in-game programming manual.
pub const META: Meta = Meta {
    id: "tin-automata",
    title: "Tin Automata",
    subtitle: "Programming puzzles",
    controls: "Left/Right Command | Up/Down Tape | Act Write | X Delete | 1 Run | 2 Step",
    accent: Color::new(0.43, 0.86, 0.77, 1.),
    help: &[
        "Program the robot to deliver every parcel. Solve twelve puzzles.",
        "In the editor, Left/Right or A/D chooses an instruction. Space writes it in the selected tape cell and advances. Writing the empty cell appends; writing an occupied cell replaces it. Up/Down selects a tape cell. Click a cell to select it; click the instruction bar to cycle the opcode.",
        "Forward moves one tile in the robot arrow's direction. Left and Right turn ninety degrees without moving. The robot cannot cross a wall, pit or closed gate. Watch its facing, especially after returning from a delivery.",
        "Grab picks up the parcel under the robot; only one can be carried. Drop puts it down on the current tile. A matching dock accepts it permanently. A wrong dock or an occupied drop tile stops the run without losing the parcel.",
        "Room five unlocks REPEAT 2: execute the previous simple instruction TWO additional times. Room six unlocks REPEAT 3 for three additional copies. Forward plus REPEAT 3 moves four tiles. Each repeated copy is a separate visible step. A repeat needs an earlier simple instruction.",
        "1 runs the tape from the restored workbench. While running, 1 pauses; press it again to resume. 2 starts a fresh attempt with one step, or steps a paused attempt. C stops and restores the board while retaining the tape. This is local puzzle control, separate from the host pause.",
        "X deletes the selected instruction, or the last instruction at the empty append cell. 3 undoes the last tape edit; the most recent 32 edits are retained. 4 clears the tape and can also be undone. These editing actions require the editor.",
        "Entering switch a permanently opens gate A for that attempt; b opens B. Stopping or retrying resets switches, parcels and the robot together. Letters and parcel numbers supplement the colors.",
        "The tape has the displayed room budget, never more than 32 cells. Execution stops at 160 steps even for malformed repetitive programs. A fault highlights the offending instruction. Press C or Space to return to editing; puzzle retries are unlimited.",
        "Three stars require no faults or hints and a program no longer than the room's par. Two stars allow four extra instructions and up to two faults; other solutions earn one. A hint caps the room at two stars. Stopping a running experiment is allowed and does not add a fault.",
        "5 shows an engineer's room-specific hint and pauses a running tape. The hint overlay closes with 5. Complete a room to bank points once, then Space opens the next. The final workbench revives the Daybreak Engine.",
        "H opens this guide; Esc/P pauses the campaign; R starts the whole factory again; M toggles sound. Completed-run records save locally when available. An unfinished factory is not saved.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
