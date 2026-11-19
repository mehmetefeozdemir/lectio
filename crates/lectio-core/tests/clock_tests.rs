//! Recording clock: start / pause / resume / stop, with time measured in
//! recorded audio samples instead of wall-clock time.

use lectio_core::clock::{ClockError, NoteDraft, RecorderCommand, RecorderPhase, RecordingClock};

#[test]
fn clock_follows_start_pause_resume_stop() {
    let clock = RecordingClock::new(16_000);
    assert_eq!(clock.phase(), RecorderPhase::Idle);

    let clock = clock.apply(RecorderCommand::Start).unwrap();
    assert_eq!(clock.phase(), RecorderPhase::Recording);
    let clock = clock.apply(RecorderCommand::Pause).unwrap();
    assert_eq!(clock.phase(), RecorderPhase::Paused);
    let clock = clock.apply(RecorderCommand::Resume).unwrap();
    assert_eq!(clock.phase(), RecorderPhase::Recording);
    let clock = clock.apply(RecorderCommand::Stop).unwrap();
    assert_eq!(clock.phase(), RecorderPhase::Stopped);
}

#[test]
fn invalid_transitions_are_errors_not_silent_no_ops() {
    let idle = RecordingClock::new(16_000);
    assert_eq!(
        idle.apply(RecorderCommand::Pause).unwrap_err(),
        ClockError::InvalidTransition { from: RecorderPhase::Idle, command: RecorderCommand::Pause }
    );
    let stopped = idle
        .apply(RecorderCommand::Start)
        .and_then(|c| c.apply(RecorderCommand::Stop))
        .unwrap();
    assert!(stopped.apply(RecorderCommand::Start).is_err());
}

#[test]
fn stop_is_allowed_while_paused() {
    let clock = RecordingClock::new(16_000)
        .apply(RecorderCommand::Start)
        .and_then(|c| c.apply(RecorderCommand::Pause))
        .and_then(|c| c.apply(RecorderCommand::Stop));
    assert_eq!(clock.unwrap().phase(), RecorderPhase::Stopped);
}

#[test]
fn offset_comes_from_recorded_samples_so_pauses_are_not_counted() {
    // 2 s recorded, then a pause (no samples arrive), then 1 s more.
    let clock = RecordingClock::new(16_000).apply(RecorderCommand::Start).unwrap();
    let clock = clock.with_total_samples(32_000);
    let clock = clock.apply(RecorderCommand::Pause).unwrap();
    assert_eq!(clock.offset_ms(), 2_000);
    let clock = clock.apply(RecorderCommand::Resume).unwrap().with_total_samples(48_000);
    assert_eq!(clock.offset_ms(), 3_000);
}

#[test]
fn sample_count_never_goes_backwards() {
    let clock = RecordingClock::new(16_000).with_total_samples(16_000).with_total_samples(8_000);
    assert_eq!(clock.offset_ms(), 1_000);
}

#[test]
fn note_draft_remembers_when_typing_started() {
    // The student starts typing at 10 s and presses Enter at 25 s:
    // the note belongs to 10 s, the moment they reacted to.
    let draft = NoteDraft::default();
    let draft = draft.with_text("B", 10_000);
    let draft = draft.with_text("Big-O", 25_000);

    assert_eq!(draft.anchor_ms(25_000), 10_000);
}

#[test]
fn clearing_the_draft_resets_the_anchor() {
    let draft = NoteDraft::default().with_text("x", 10_000).with_text("", 12_000);
    assert_eq!(draft.anchor_ms(30_000), 30_000);
}
