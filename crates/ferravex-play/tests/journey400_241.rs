//! Public consumer regression: outcome APIs retain the buffer's Copy-only bound.
use ferravex_play::gameplay::{
    LoopError,
    buffer::{AdvanceOutcome, CommandBuffer, ConsumeOutcome, PushOutcome},
};

#[derive(Clone, Copy)]
struct Command {
    id: u8,
    impulse: f32,
}

#[test]
fn opaque_copy_payloads_are_returned_intact_without_equality_or_debug_bounds() {
    let mut commands = CommandBuffer::new(0.125).unwrap();
    let impulse = f32::from_bits(0x7fc0_0241);
    assert!(matches!(
        commands.push_checked(Command { id: 1, impulse }),
        PushOutcome::Queued
    ));
    let PushOutcome::Replaced(old) = commands.push_checked(Command { id: 2, impulse }) else {
        panic!("the second edge replaces the first");
    };
    assert_eq!((old.id, old.impulse.to_bits()), (1, impulse.to_bits()));
    assert_eq!(
        commands.pending().unwrap().impulse.to_bits(),
        impulse.to_bits()
    );
    assert!(matches!(
        commands.take_if_checked(false),
        ConsumeOutcome::Gated
    ));
    assert!(matches!(
        commands.advance_checked(f32::NAN),
        Err(LoopError::Invalid)
    ));
    assert_eq!(commands.remaining_life(), Some(0.125));
    let AdvanceOutcome::Expired(expired) = commands.advance_checked(0.125).unwrap() else {
        panic!("the replacement expires once");
    };
    assert_eq!(
        (expired.id, expired.impulse.to_bits()),
        (2, impulse.to_bits())
    );
    assert!(matches!(
        commands.advance_checked(0.).unwrap(),
        AdvanceOutcome::Empty
    ));
    commands.push(Command { id: 3, impulse });
    let ConsumeOutcome::Executed(executed) = commands.take_if_checked(true) else {
        panic!("a legacy push can be consumed through the checked API");
    };
    assert_eq!(
        (executed.id, executed.impulse.to_bits()),
        (3, impulse.to_bits())
    );
    assert_eq!(commands.remaining_life(), None);
    assert!(commands.take_if(true).is_none());
}
