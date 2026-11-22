use nookery_audio::{AudioCommand, AudioCommandQueue, QueueError, StereoFrame, StereoRing};

fn frame(value: i16) -> StereoFrame {
    StereoFrame {
        left: value,
        right: -value,
    }
}

#[test]
fn fifo_order_and_partial_reads_are_exact() {
    let mut ring = StereoRing::new(4);
    assert_eq!(
        ring.push(&[frame(1), frame(2), frame(3)]).accepted_frames,
        3
    );

    let mut first = [StereoFrame::SILENCE; 2];
    ring.pop(&mut first);
    assert_eq!(first, [frame(1), frame(2)]);

    assert_eq!(ring.push(&[frame(4), frame(5)]).accepted_frames, 2);
    let mut second = [StereoFrame::SILENCE; 3];
    ring.pop(&mut second);
    assert_eq!(second, [frame(3), frame(4), frame(5)]);
    assert!(ring.is_empty());
}

#[test]
fn underflow_returns_silence_and_counts_frames() {
    let mut ring = StereoRing::new(2);
    let mut output = [frame(9); 3];
    ring.pop(&mut output);
    assert_eq!(output, [StereoFrame::SILENCE; 3]);
    assert_eq!(ring.underflow_frames(), 3);
}

#[test]
fn overflow_drops_newest_and_counts_frames() {
    let mut ring = StereoRing::new(2);
    let report = ring.push(&[frame(1), frame(2), frame(3)]);
    assert_eq!(report.accepted_frames, 2);
    assert_eq!(report.dropped_frames, 1);
    assert_eq!(ring.dropped_frames(), 1);

    let mut output = [StereoFrame::SILENCE; 2];
    ring.pop(&mut output);
    assert_eq!(output, [frame(1), frame(2)]);
}

#[test]
fn shutdown_clears_pending_commands() {
    let mut queue = AudioCommandQueue::new(2);
    queue.try_push(AudioCommand::UpdateDac).unwrap();
    queue.try_push(AudioCommand::DspSync).unwrap();
    queue.shutdown();

    assert_eq!(queue.len(), 0);
    assert_eq!(queue.stats().cleared_commands, 2);
    assert_eq!(queue.try_pop(), Err(QueueError::Shutdown));
    assert_eq!(
        queue.try_push(AudioCommand::NeosSync),
        Err(QueueError::Shutdown)
    );

    queue.reset();
    assert_eq!(queue.try_push(AudioCommand::NeosSync), Ok(()));
    assert_eq!(queue.try_pop(), Ok(AudioCommand::NeosSync));
}

#[test]
fn capacity_and_wraparound_preserve_order() {
    let mut ring = StereoRing::new(3);
    assert_eq!(ring.capacity(), 3);
    assert_eq!(
        ring.push(&[frame(1), frame(2), frame(3), frame(4)])
            .dropped_frames,
        1
    );

    let mut first = [StereoFrame::SILENCE; 2];
    ring.pop(&mut first);
    assert_eq!(first, [frame(1), frame(2)]);
    assert_eq!(ring.push(&[frame(5), frame(6)]).accepted_frames, 2);

    let mut rest = [StereoFrame::SILENCE; 3];
    ring.pop(&mut rest);
    assert_eq!(rest, [frame(3), frame(5), frame(6)]);
    assert_eq!(ring.len(), 0);
}
