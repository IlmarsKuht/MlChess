use super::*;
use std::thread;

#[test]
fn test_search_limits_depth_only() {
    let limits = SearchLimits::depth(5);
    assert_eq!(limits.depth, 5);
    assert!(limits.move_time.is_none());
    assert!(!limits.should_stop());
}

#[test]
fn test_search_limits_with_time() {
    let limits = SearchLimits::depth_and_time(4, Duration::from_millis(100));
    assert_eq!(limits.depth, 4);
    assert_eq!(limits.move_time, Some(Duration::from_millis(100)));
}

#[test]
fn test_time_control_expiry() {
    let tc = TimeControl::new(Some(Duration::from_millis(10)));
    tc.start();
    assert!(!tc.is_stopped());

    // Wait for time to expire
    thread::sleep(Duration::from_millis(20));
    tc.check_time();
    assert!(tc.is_stopped());
}

#[test]
fn test_time_control_no_limit() {
    let tc = TimeControl::new(None);
    tc.start();
    thread::sleep(Duration::from_millis(10));
    tc.check_time();
    assert!(!tc.is_stopped());
}

#[test]
fn test_time_control_manual_stop() {
    let tc = TimeControl::new(None);
    tc.start();
    assert!(!tc.is_stopped());
    tc.stop();
    assert!(tc.is_stopped());
}
