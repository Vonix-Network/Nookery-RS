//! Borealis/Aurora-compatible stream lifecycle vectors.

use std::io::{Cursor, SeekFrom};

use nookery_disc::{DiscImage, DiscStream, ImageError, StreamError};

#[test]
fn open_read_seek_tell_and_absolute_read_follow_callback_semantics() {
    let mut stream = DiscStream::open(Cursor::new(b"0123456789".to_vec())).expect("open");
    assert_eq!(stream.len(), 10);
    assert_eq!(stream.tell(), 0);

    let mut bytes = [0u8; 3];
    assert_eq!(stream.read(&mut bytes).expect("read"), 3);
    assert_eq!(&bytes, b"012");
    assert_eq!(stream.tell(), 3);

    assert_eq!(stream.seek(SeekFrom::Start(7)).expect("seek"), 7);
    assert_eq!(stream.tell(), 7);
    assert_eq!(stream.read_at(4, &mut bytes).expect("read_at"), 3);
    assert_eq!(&bytes, b"456");
    assert_eq!(stream.tell(), 7);

    assert_eq!(stream.seek(SeekFrom::End(0)).expect("tell at end"), 10);
    assert_eq!(stream.read(&mut bytes).expect("EOF read"), 0);
}

#[test]
fn close_is_idempotent_and_rejects_following_operations() {
    let mut stream = DiscStream::open(Cursor::new(b"x".to_vec())).expect("open");
    stream.close().expect("close");
    stream.close().expect("second close");
    let mut byte = [0u8; 1];
    assert!(matches!(stream.read(&mut byte), Err(StreamError::Closed)));
    assert!(matches!(
        stream.seek(SeekFrom::Start(0)),
        Err(StreamError::Closed)
    ));
}

#[test]
fn disc_image_adapter_reports_eof_without_overreading() {
    let mut stream = DiscStream::open(Cursor::new(b"abc".to_vec())).expect("open");
    let mut output = [0u8; 8];
    assert_eq!(stream.read_at(2, &mut output).expect("tail read"), 1);
    assert_eq!(&output[..1], b"c");
    assert_eq!(stream.read_at(3, &mut output).expect("EOF read"), 0);
    assert_eq!(
        <DiscStream<_> as DiscImage>::read_at(&mut stream, u64::MAX, &mut output),
        Ok(0)
    );
    assert_eq!(stream.logical_size(), 3);
}

#[test]
fn closed_disc_image_is_a_failed_backend_not_a_short_success() {
    let mut stream = DiscStream::open(Cursor::new(b"abc".to_vec())).expect("open");
    stream.close().expect("close");
    let mut output = [0u8; 1];
    assert_eq!(
        <DiscStream<_> as DiscImage>::read_at(&mut stream, 0, &mut output),
        Err(ImageError::Failed)
    );
}
