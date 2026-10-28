//! Decision logic for the disc startup menu watcher.
//!
//! After the initial menu request, the watcher thread in `commands::media`
//! samples the player every [`POLL_MS`] for up to [`MAX_POLLS`] polls and asks
//! [`decide`] what to do next. The thread owns all side effects (libVLC calls,
//! session checks, the fallback); this module only decides, so the startup
//! rules can be tested without a player.
//!
//! The watcher is bounded and cannot re-trigger itself. It only exists for a
//! session opened with menus (`bypass_menu == false`), runs once per such
//! open, and its single fallback reopens the disc with
//! [`FALLBACK_BYPASS_MENU`]; a session opened that way never starts a watcher
//! ([`startup_watchers_enabled`]).

use super::engine::{first_play_skip_allowed, TitleKind};

/// Poll cadence after the initial menu request.
pub const POLL_MS: u64 = 200;
/// The menu request is repeated every this many polls (2 s).
pub const RETRY_EVERY_POLLS: u32 = 10;
/// The watcher gives up after this many polls (15 s).
pub const MAX_POLLS: u32 = 75;

/// `bypass_menu` of the session the fallback reopens.
pub const FALLBACK_BYPASS_MENU: bool = true;

/// Does a session opened with this `bypass_menu` get the disc startup
/// watchers (menu watcher and warning-skip watcher)?
pub const fn startup_watchers_enabled(bypass_menu: bool) -> bool {
    !bypass_menu
}

/// What the watcher sees at one poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// The current title is the authored Top Menu.
    pub at_menu: bool,
    /// The disc's title list contains an authored menu title.
    pub has_menu: bool,
    /// What VLC's title list says the current title is.
    pub kind: TitleKind,
    /// A feature title is actively playing.
    pub feature_active: bool,
}

/// State carried between polls of one watcher run.
#[derive(Debug)]
pub struct Progress {
    skipped_first_play: bool,
    retry_after_skip_pending: bool,
    next_cadence_poll: u32,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            skipped_first_play: false,
            retry_after_skip_pending: false,
            next_cadence_poll: RETRY_EVERY_POLLS,
        }
    }
}

/// What the watcher thread should do at this poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do until the next poll.
    Wait,
    /// The authored menu is showing; stop watching.
    Reached,
    /// The disc has no authored menu and a feature is playing; stop watching
    /// and let it play.
    PlainPlayback,
    /// Seek First Play to its end, once, so the disc leaves it.
    SkipFirstPlay,
    /// Send `navigate_menu` and `set_title(menu title)` again.
    RetryMenu,
}

/// Decide the next step for poll number `poll` (0-based).
///
/// - A feature title becoming active does not end the watch while the disc has
///   an authored menu that has not appeared yet; only a disc without a menu
///   title takes the plain-playback exit.
/// - First Play is skipped at most once per run, and only when
///   [`first_play_skip_allowed`] says so.
/// - Once the title has left First Play after that skip, the menu is requested
///   again right away (the menu call was masked only while First Play ran);
///   otherwise requests repeat on the fixed cadence.
pub fn decide(progress: &mut Progress, poll: u32, s: Sample) -> Decision {
    if s.at_menu {
        return Decision::Reached;
    }
    if !s.has_menu && s.feature_active {
        return Decision::PlainPlayback;
    }
    if first_play_skip_allowed(
        s.has_menu,
        s.kind,
        s.feature_active,
        progress.skipped_first_play,
    ) {
        progress.skipped_first_play = true;
        progress.retry_after_skip_pending = true;
        return Decision::SkipFirstPlay;
    }
    if progress.retry_after_skip_pending && matches!(s.kind, TitleKind::Other { .. }) {
        progress.retry_after_skip_pending = false;
        return Decision::RetryMenu;
    }
    if poll >= progress.next_cadence_poll {
        progress.next_cadence_poll += RETRY_EVERY_POLLS;
        return Decision::RetryMenu;
    }
    Decision::Wait
}

/// What the watcher does when its bounded window ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEnd {
    /// The authored menu is showing after all.
    AtMenu,
    /// Something worth keeping is playing; leave it alone.
    KeepPlaying,
    /// Nothing usable is playing: reopen the disc once as direct playback
    /// (when the caller knows the disc path).
    DirectFeature,
}

/// Decide how the watcher ends once [`MAX_POLLS`] polls found no menu.
/// `keep_playing` means a title is playing that is worth preserving (a
/// feature, or a title long enough to be one); the caller defines it.
///
/// `DirectFeature` is the only outcome that acts, and it needs the menu to be
/// absent and nothing worth keeping to be playing, so neither is replaced.
pub fn window_end(at_menu: bool, keep_playing: bool) -> WindowEnd {
    if at_menu {
        WindowEnd::AtMenu
    } else if keep_playing {
        WindowEnd::KeepPlaying
    } else {
        WindowEnd::DirectFeature
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shortest title `engine::is_active_feature_playback` calls a feature.
    const FEATURE_MIN_MS: i64 = 120_000;

    const FIRST_PLAY: TitleKind = TitleKind::FirstPlay {
        duration_ms: 36_035,
    };
    const FEATURE: TitleKind = TitleKind::Other {
        duration_ms: 5_350_345,
    };

    fn sample(kind: TitleKind) -> Sample {
        Sample {
            at_menu: kind == TitleKind::Menu,
            has_menu: true,
            kind,
            feature_active: matches!(kind, TitleKind::Other { duration_ms } if duration_ms >= FEATURE_MIN_MS),
        }
    }

    /// Run the watcher loop over a scripted disc: `script(poll)` is what the
    /// player reports at that poll. Returns every non-`Wait` decision and
    /// stops at the first terminal one, like the real thread does.
    fn run(mut script: impl FnMut(u32) -> Sample) -> Vec<(u32, Decision)> {
        let mut progress = Progress::default();
        let mut out = Vec::new();
        for poll in 0..MAX_POLLS {
            let d = decide(&mut progress, poll, script(poll));
            if d != Decision::Wait {
                out.push((poll, d));
            }
            if matches!(d, Decision::Reached | Decision::PlainPlayback) {
                break;
            }
        }
        out
    }

    #[test]
    fn timing_constants_match_the_documented_bounds() {
        assert_eq!(POLL_MS * u64::from(MAX_POLLS), 15_000);
        assert_eq!(POLL_MS * u64::from(RETRY_EVERY_POLLS), 2_000);
    }

    #[test]
    fn reaching_the_menu_always_stops_the_watch() {
        let mut p = Progress::default();
        for kind in [TitleKind::Menu, TitleKind::Unknown, FIRST_PLAY, FEATURE] {
            let s = Sample {
                at_menu: true,
                ..sample(kind)
            };
            assert_eq!(decide(&mut p, 5, s), Decision::Reached);
        }
    }

    #[test]
    fn menuless_disc_with_a_feature_playing_takes_the_plain_playback_exit() {
        let s = Sample {
            has_menu: false,
            ..sample(FEATURE)
        };
        assert_eq!(
            decide(&mut Progress::default(), 3, s),
            Decision::PlainPlayback
        );
    }

    #[test]
    fn disc_with_a_menu_keeps_trying_while_a_feature_plays() {
        // The feature title is active and the menu has not appeared: no
        // plain-playback exit, requests continue on the 2 s cadence.
        let events = run(|_| sample(FEATURE));
        let polls: Vec<u32> = events.iter().map(|e| e.0).collect();
        assert_eq!(polls, vec![10, 20, 30, 40, 50, 60, 70]);
        assert!(events.iter().all(|e| e.1 == Decision::RetryMenu));
    }

    #[test]
    fn first_play_is_skipped_exactly_once() {
        let events = run(|_| sample(FIRST_PLAY));
        let skips = events
            .iter()
            .filter(|e| e.1 == Decision::SkipFirstPlay)
            .count();
        assert_eq!(skips, 1);
        assert_eq!(events[0], (0, Decision::SkipFirstPlay));
    }

    #[test]
    fn first_play_with_unknown_length_waits_then_skips_once_known() {
        let events = run(|poll| {
            sample(TitleKind::FirstPlay {
                duration_ms: if poll < 3 { 0 } else { 36_035 },
            })
        });
        assert_eq!(events[0], (3, Decision::SkipFirstPlay));
        assert_eq!(
            events
                .iter()
                .filter(|e| e.1 == Decision::SkipFirstPlay)
                .count(),
            1
        );
    }

    #[test]
    fn first_play_that_never_reports_a_length_is_never_skipped() {
        // Mary-type disc: the First Play length stays 0, so the seek would be
        // a no-op. Only cadence requests run, then the caller's bounded exit.
        let events = run(|_| sample(TitleKind::FirstPlay { duration_ms: 0 }));
        assert!(events.iter().all(|e| e.1 == Decision::RetryMenu));
        let polls: Vec<u32> = events.iter().map(|e| e.0).collect();
        assert_eq!(polls, vec![10, 20, 30, 40, 50, 60, 70]);
    }

    #[test]
    fn no_skip_without_a_positive_first_play_identification() {
        for kind in [
            TitleKind::Unknown,
            TitleKind::Other {
                duration_ms: 60_000,
            },
            TitleKind::Other { duration_ms: 0 },
        ] {
            let events = run(|_| Sample {
                feature_active: false,
                ..sample(kind)
            });
            assert!(
                events.iter().all(|e| e.1 != Decision::SkipFirstPlay),
                "{kind:?} must not be skipped"
            );
        }
    }

    #[test]
    fn no_skip_when_the_disc_has_no_menu_or_a_feature_is_active() {
        let no_menu = run(|_| Sample {
            has_menu: false,
            ..sample(FIRST_PLAY)
        });
        assert!(no_menu.iter().all(|e| e.1 != Decision::SkipFirstPlay));
        let feature = run(|_| Sample {
            feature_active: true,
            ..sample(FIRST_PLAY)
        });
        assert!(feature.iter().all(|e| e.1 != Decision::SkipFirstPlay));
    }

    #[test]
    fn leaving_first_play_after_the_skip_requests_the_menu_promptly() {
        // Gundam Narrative as logged: First Play, seek lands on the feature
        // title, the menu call works once First Play is gone.
        let events = run(|poll| match poll {
            0..=1 => sample(FIRST_PLAY),
            2..=11 => sample(FEATURE),
            _ => sample(TitleKind::Menu),
        });
        assert_eq!(
            events,
            vec![
                (0, Decision::SkipFirstPlay),
                (2, Decision::RetryMenu),
                (10, Decision::RetryMenu),
                (12, Decision::Reached),
            ]
        );
    }

    #[test]
    fn prompt_retry_happens_once_per_skip_and_the_cadence_continues() {
        let events = run(|poll| {
            if poll == 0 {
                sample(FIRST_PLAY)
            } else {
                sample(FEATURE)
            }
        });
        let polls: Vec<u32> = events.iter().map(|e| e.0).collect();
        assert_eq!(polls, vec![0, 1, 10, 20, 30, 40, 50, 60, 70]);
    }

    #[test]
    fn a_skip_that_does_not_leave_first_play_is_not_repeated() {
        // The seek had no effect: First Play keeps running for the whole
        // bound. One skip, then only cadence requests; the caller's fallback
        // takes over after MAX_POLLS.
        let events = run(|_| sample(FIRST_PLAY));
        let kinds: Vec<Decision> = events.iter().map(|e| e.1).collect();
        assert_eq!(kinds[0], Decision::SkipFirstPlay);
        assert!(kinds[1..].iter().all(|d| *d == Decision::RetryMenu));
        let polls: Vec<u32> = events.iter().skip(1).map(|e| e.0).collect();
        assert_eq!(polls, vec![10, 20, 30, 40, 50, 60, 70]);
    }

    #[test]
    fn first_play_returning_after_the_skip_is_not_skipped_again() {
        let events = run(|poll| match poll {
            0 => sample(FIRST_PLAY),
            1..=4 => sample(FEATURE),
            _ => sample(FIRST_PLAY),
        });
        assert_eq!(
            events
                .iter()
                .filter(|e| e.1 == Decision::SkipFirstPlay)
                .count(),
            1
        );
    }

    #[test]
    fn requests_never_exceed_the_bound() {
        let events = run(|_| sample(FEATURE));
        assert!(events.iter().all(|e| e.0 < MAX_POLLS));
        assert!(events.len() <= (MAX_POLLS / RETRY_EVERY_POLLS) as usize + 2);
    }

    #[test]
    fn window_end_never_replaces_a_menu_or_a_playing_title() {
        assert_eq!(window_end(true, true), WindowEnd::AtMenu);
        assert_eq!(window_end(true, false), WindowEnd::AtMenu);
        assert_eq!(window_end(false, true), WindowEnd::KeepPlaying);
    }

    #[test]
    fn window_end_falls_back_only_when_nothing_usable_plays() {
        assert_eq!(window_end(false, false), WindowEnd::DirectFeature);
    }

    #[test]
    fn the_fallback_session_never_starts_a_startup_watcher() {
        // The fallback reopens the disc once. The reopened session must not
        // run the watcher again, or an unreachable menu would loop
        // open -> 15 s -> reopen.
        assert!(!startup_watchers_enabled(FALLBACK_BYPASS_MENU));
        assert!(startup_watchers_enabled(false));
    }
}
