//! Decision logic for the disc startup menu watcher.
//!
//! After the initial menu request, the watcher thread in `commands::media`
//! samples the player and asks [`decide`] what to do next. The thread owns all
//! side effects (libVLC calls, session checks, the fallback); this module only
//! decides, so the startup rules can be tested without a player.
//!
//! The watcher is bounded and cannot re-trigger itself. It only exists for a
//! session opened with menus (`bypass_menu == false`), runs once per such
//! open for at most [`WINDOW_MS`], and its single fallback reopens the disc
//! with [`FALLBACK_BYPASS_MENU`]; a session opened that way never starts a
//! watcher ([`startup_watchers_enabled`]).
//!
//! libVLC's current-title report is not proof that the disc shows its menu.
//! `libvlc_media_player_set_title` makes the input thread announce the
//! requested title whether or not the Blu-ray VM accepted it (3.0.x
//! `input.c`, `INPUT_CONTROL_SET_TITLE`). While First Play runs with the menu
//! call masked, that false "menu" report holds until the demuxer's next title
//! event, about half a second later (Gundam Narrative and Patlabor 2 UHD, B3
//! logs). A menu report therefore only ends the watch once it has held for
//! [`MENU_CONFIRM_MS`].

use super::engine::{first_play_skip_allowed, TitleKind};

/// Poll cadence.
pub const POLL_MS: u64 = 200;
/// Poll cadence while the watcher waits for the disc to leave First Play after
/// the skip, so the menu request follows the title change within a few
/// milliseconds instead of up to a whole [`POLL_MS`]. The feature that First
/// Play jumps to is on screen from that title change until the menu call
/// lands.
pub const FAST_POLL_MS: u64 = 25;
/// The fast cadence lasts at most this long after the skip.
pub const FAST_POLL_WINDOW_MS: u64 = 4_000;
/// The menu request is repeated on this grid (2 s).
pub const RETRY_EVERY_MS: u64 = 2_000;
/// The watcher gives up after this long (15 s).
pub const WINDOW_MS: u64 = 15_000;
/// A "menu" report must hold this long before it ends the watch. It is longer
/// than the roughly half-second a rejected `set_title` report survives.
pub const MENU_CONFIRM_MS: u64 = 1_000;

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
    skipped_at_ms: Option<u64>,
    retry_after_skip_pending: bool,
    menu_since_ms: Option<u64>,
    next_cadence_ms: u64,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            skipped_at_ms: None,
            retry_after_skip_pending: false,
            menu_since_ms: None,
            next_cadence_ms: RETRY_EVERY_MS,
        }
    }
}

impl Progress {
    /// How long to sleep before the next poll, given the time of the last one.
    pub fn next_poll_ms(&self, now_ms: u64) -> u64 {
        match self.skipped_at_ms {
            Some(at)
                if self.retry_after_skip_pending
                    && now_ms.saturating_sub(at) < FAST_POLL_WINDOW_MS =>
            {
                FAST_POLL_MS
            }
            _ => POLL_MS,
        }
    }

    /// Start of the current unbroken run of "menu" reports, if one is running.
    pub fn menu_claim_since_ms(&self) -> Option<u64> {
        self.menu_since_ms
    }
}

/// What the watcher thread should do at this poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do until the next poll.
    Wait,
    /// The authored menu has been showing for [`MENU_CONFIRM_MS`]; stop
    /// watching.
    Reached,
    /// The disc has no authored menu and a feature is playing; stop watching
    /// and let it play.
    PlainPlayback,
    /// Seek First Play to its end, once, so the disc leaves it.
    SkipFirstPlay,
    /// Send `navigate_menu` and `set_title(menu title)` again.
    RetryMenu,
}

/// Decide the next step for the poll at `now_ms` (milliseconds since the
/// watch started).
///
/// - A menu report ends the watch only after it has held without a gap for
///   [`MENU_CONFIRM_MS`]; until then the watcher waits and sends no further
///   menu requests. A report that disappears (the VM refused the request) is
///   forgotten.
/// - A feature title becoming active does not end the watch while the disc has
///   an authored menu that has not appeared yet; only a disc without a menu
///   title takes the plain-playback exit.
/// - First Play is skipped at most once per run, and only when
///   [`first_play_skip_allowed`] says so.
/// - Once the title has left First Play after that skip, the menu is requested
///   again right away (the menu call was masked only while First Play ran);
///   otherwise requests repeat on the fixed [`RETRY_EVERY_MS`] grid.
pub fn decide(progress: &mut Progress, now_ms: u64, s: Sample) -> Decision {
    if s.at_menu {
        let since = *progress.menu_since_ms.get_or_insert(now_ms);
        return if now_ms.saturating_sub(since) >= MENU_CONFIRM_MS {
            Decision::Reached
        } else {
            Decision::Wait
        };
    }
    progress.menu_since_ms = None;

    if !s.has_menu && s.feature_active {
        return Decision::PlainPlayback;
    }
    if first_play_skip_allowed(
        s.has_menu,
        s.kind,
        s.feature_active,
        progress.skipped_at_ms.is_some(),
    ) {
        progress.skipped_at_ms = Some(now_ms);
        progress.retry_after_skip_pending = true;
        return Decision::SkipFirstPlay;
    }
    if progress.retry_after_skip_pending && matches!(s.kind, TitleKind::Other { .. }) {
        progress.retry_after_skip_pending = false;
        return Decision::RetryMenu;
    }
    if now_ms >= progress.next_cadence_ms {
        progress.next_cadence_ms = (now_ms / RETRY_EVERY_MS + 1) * RETRY_EVERY_MS;
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

/// Decide how the watcher ends once [`WINDOW_MS`] passed without a confirmed
/// menu. `keep_playing` means a title is playing that is worth preserving (a
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
    const FIRST_PLAY_UNKNOWN: TitleKind = TitleKind::FirstPlay { duration_ms: 0 };
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

    /// Run the watcher loop over a scripted disc, the way the real thread
    /// does: sleep [`Progress::next_poll_ms`], sample, decide. `script(t)` is
    /// what the player reports `t` ms after the watch started. Returns every
    /// non-`Wait` decision with its time and stops at the first terminal one.
    fn run(mut script: impl FnMut(u64) -> Sample) -> Vec<(u64, Decision)> {
        let mut progress = Progress::default();
        let mut out = Vec::new();
        let mut now = 0;
        loop {
            now += progress.next_poll_ms(now);
            if now > WINDOW_MS {
                break;
            }
            let d = decide(&mut progress, now, script(now));
            if d != Decision::Wait {
                out.push((now, d));
            }
            if matches!(d, Decision::Reached | Decision::PlainPlayback) {
                break;
            }
        }
        out
    }

    fn decisions(events: &[(u64, Decision)]) -> Vec<Decision> {
        events.iter().map(|e| e.1).collect()
    }

    fn times(events: &[(u64, Decision)]) -> Vec<u64> {
        events.iter().map(|e| e.0).collect()
    }

    fn skips(events: &[(u64, Decision)]) -> usize {
        events
            .iter()
            .filter(|e| e.1 == Decision::SkipFirstPlay)
            .count()
    }

    /// B3 logs (Gundam Narrative, session 14): the false "menu" report from a
    /// rejected `set_title` lived from +25 ms to +486 ms.
    const OBSERVED_FALSE_REPORT_MS: u64 = 486 - 25;

    // The confirmation must outlast the false report with a wide margin, and
    // the fast cadence must really be faster.
    const _: () = assert!(MENU_CONFIRM_MS >= 2 * OBSERVED_FALSE_REPORT_MS);
    const _: () = assert!(FAST_POLL_MS < POLL_MS);

    #[test]
    fn timing_constants_match_the_documented_bounds() {
        assert_eq!(WINDOW_MS, 15_000);
        assert_eq!(RETRY_EVERY_MS, 2_000);
        assert_eq!(WINDOW_MS % POLL_MS, 0);
        assert_eq!(RETRY_EVERY_MS % POLL_MS, 0);
    }

    #[test]
    fn a_steady_menu_ends_the_watch_after_the_confirmation_time() {
        let events = run(|_| sample(TitleKind::Menu));
        assert_eq!(events, vec![(POLL_MS + MENU_CONFIRM_MS, Decision::Reached)]);
    }

    #[test]
    fn reaching_the_menu_stops_the_watch_whatever_else_is_reported() {
        for kind in [TitleKind::Menu, TitleKind::Unknown, FIRST_PLAY, FEATURE] {
            let mut p = Progress::default();
            let s = Sample {
                at_menu: true,
                ..sample(kind)
            };
            // The first report only starts the confirmation; one that has
            // held long enough ends the watch.
            assert_eq!(decide(&mut p, 5_000, s), Decision::Wait);
            assert_eq!(
                decide(&mut p, 5_000 + MENU_CONFIRM_MS, s),
                Decision::Reached
            );
        }
    }

    #[test]
    fn a_rejected_set_title_that_reverts_to_first_play_is_not_reached() {
        // Gundam Narrative as logged in B3 (session 14): the menu request is
        // rejected while First Play masks it, yet libVLC reports the menu for
        // about 450 ms. The old watcher ended on that first sample and left
        // First Play running for 38 s. The claim must be forgotten, and First
        // Play skipped once its length is known.
        let events = run(|t| match t {
            0..=599 => sample(TitleKind::Menu),
            600..=799 => sample(FIRST_PLAY_UNKNOWN),
            _ => sample(FIRST_PLAY),
        });
        assert!(!decisions(&events).contains(&Decision::Reached));
        assert_eq!(events[0], (800, Decision::SkipFirstPlay));
    }

    #[test]
    fn a_menu_report_that_flickers_never_accumulates_toward_the_confirmation() {
        // Reported for 800 ms, gone for one poll, reported again: the second
        // run starts from zero, so the watch is not ended by their sum.
        let events = run(|t| {
            if (1_000..1_200).contains(&t) {
                sample(FIRST_PLAY_UNKNOWN)
            } else {
                sample(TitleKind::Menu)
            }
        });
        assert_eq!(events, vec![(1_200 + MENU_CONFIRM_MS, Decision::Reached)]);
    }

    #[test]
    fn no_menu_requests_are_sent_while_a_menu_report_is_being_confirmed() {
        // A cadence tick that falls inside the confirmation window must not
        // re-send the request; that would restart a menu that is showing.
        let events = run(|t| {
            if t >= 1_600 {
                sample(TitleKind::Menu)
            } else {
                sample(FEATURE)
            }
        });
        // The report starts at 1600 ms; the grid tick at 2000 ms falls inside
        // the confirmation window and sends nothing.
        assert_eq!(events, vec![(1_600 + MENU_CONFIRM_MS, Decision::Reached)]);
    }

    #[test]
    fn menuless_disc_with_a_feature_playing_takes_the_plain_playback_exit() {
        let s = Sample {
            has_menu: false,
            ..sample(FEATURE)
        };
        assert_eq!(
            decide(&mut Progress::default(), 600, s),
            Decision::PlainPlayback
        );
    }

    #[test]
    fn disc_with_a_menu_keeps_trying_while_a_feature_plays() {
        // The feature title is active and the menu has not appeared: no
        // plain-playback exit, requests continue on the 2 s grid.
        let events = run(|_| sample(FEATURE));
        assert_eq!(
            times(&events),
            vec![2_000, 4_000, 6_000, 8_000, 10_000, 12_000, 14_000]
        );
        assert!(events.iter().all(|e| e.1 == Decision::RetryMenu));
    }

    #[test]
    fn first_play_is_skipped_exactly_once() {
        let events = run(|_| sample(FIRST_PLAY));
        assert_eq!(skips(&events), 1);
        assert_eq!(events[0], (POLL_MS, Decision::SkipFirstPlay));
    }

    #[test]
    fn first_play_with_unknown_length_waits_then_skips_once_known() {
        let events = run(|t| {
            sample(TitleKind::FirstPlay {
                duration_ms: if t < 600 { 0 } else { 36_035 },
            })
        });
        assert_eq!(events[0], (600, Decision::SkipFirstPlay));
        assert_eq!(skips(&events), 1);
    }

    #[test]
    fn first_play_that_never_reports_a_length_is_never_skipped() {
        // Mary-type disc: the First Play length stays 0, so the seek would be
        // a no-op. Only grid requests run, then the caller's bounded exit.
        let events = run(|_| sample(FIRST_PLAY_UNKNOWN));
        assert!(events.iter().all(|e| e.1 == Decision::RetryMenu));
        assert_eq!(
            times(&events),
            vec![2_000, 4_000, 6_000, 8_000, 10_000, 12_000, 14_000]
        );
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
            assert_eq!(skips(&events), 0, "{kind:?} must not be skipped");
        }
    }

    #[test]
    fn no_skip_when_the_disc_has_no_menu_or_a_feature_is_active() {
        let no_menu = run(|_| Sample {
            has_menu: false,
            ..sample(FIRST_PLAY)
        });
        assert_eq!(skips(&no_menu), 0);
        let feature = run(|_| Sample {
            feature_active: true,
            ..sample(FIRST_PLAY)
        });
        assert_eq!(skips(&feature), 0);
    }

    #[test]
    fn leaving_first_play_after_the_skip_requests_the_menu_promptly() {
        // Gundam Narrative as logged (session 16): First Play, the seek lands
        // on the feature title, the menu call works once First Play is gone.
        // The watcher polls fast after the skip, so the request goes out
        // within FAST_POLL_MS of the title change.
        let events = run(|t| match t {
            0..=699 => sample(FIRST_PLAY),
            700..=1_699 => sample(FEATURE),
            _ => sample(TitleKind::Menu),
        });
        let skip_at = POLL_MS;
        let retry = events
            .iter()
            .find(|e| e.1 == Decision::RetryMenu)
            .expect("a prompt retry");
        assert_eq!(events[0], (skip_at, Decision::SkipFirstPlay));
        assert!(
            retry.0 >= 700 && retry.0 < 700 + 2 * FAST_POLL_MS,
            "retry at {} ms",
            retry.0
        );
        assert_eq!(events.last().map(|e| e.1), Some(Decision::Reached));
    }

    #[test]
    fn the_wait_for_the_title_change_is_the_only_fast_polled_phase() {
        let mut p = Progress::default();
        // Before the skip: normal cadence.
        assert_eq!(p.next_poll_ms(0), POLL_MS);
        assert_eq!(
            decide(&mut p, POLL_MS, sample(FIRST_PLAY)),
            Decision::SkipFirstPlay
        );
        // Waiting for First Play to end: fast, until the window closes.
        assert_eq!(p.next_poll_ms(POLL_MS), FAST_POLL_MS);
        assert_eq!(
            p.next_poll_ms(POLL_MS + FAST_POLL_WINDOW_MS - 1),
            FAST_POLL_MS
        );
        assert_eq!(p.next_poll_ms(POLL_MS + FAST_POLL_WINDOW_MS), POLL_MS);
        // Once the retry went out: back to the normal cadence.
        assert_eq!(decide(&mut p, 900, sample(FEATURE)), Decision::RetryMenu);
        assert_eq!(p.next_poll_ms(900), POLL_MS);
    }

    #[test]
    fn prompt_retry_happens_once_per_skip_and_the_grid_continues() {
        let events = run(|t| {
            if t < 400 {
                sample(FIRST_PLAY)
            } else {
                sample(FEATURE)
            }
        });
        let kinds = decisions(&events);
        assert_eq!(kinds[0], Decision::SkipFirstPlay);
        assert_eq!(kinds[1], Decision::RetryMenu);
        assert!(events[1].0 < 400 + 2 * FAST_POLL_MS);
        // Everything after the prompt retry is on the 2 s grid.
        assert_eq!(
            times(&events[2..]),
            vec![2_000, 4_000, 6_000, 8_000, 10_000, 12_000, 14_000]
        );
    }

    #[test]
    fn a_skip_that_does_not_leave_first_play_is_not_repeated() {
        // The seek had no effect: First Play keeps running for the whole
        // bound. One skip, then only grid requests; the caller's fallback
        // takes over after WINDOW_MS.
        let events = run(|_| sample(FIRST_PLAY));
        let kinds = decisions(&events);
        assert_eq!(kinds[0], Decision::SkipFirstPlay);
        assert!(kinds[1..].iter().all(|d| *d == Decision::RetryMenu));
        assert_eq!(
            times(&events[1..]),
            vec![2_000, 4_000, 6_000, 8_000, 10_000, 12_000, 14_000]
        );
    }

    #[test]
    fn first_play_returning_after_the_skip_is_not_skipped_again() {
        let events = run(|t| match t {
            0..=299 => sample(FIRST_PLAY),
            300..=999 => sample(FEATURE),
            _ => sample(FIRST_PLAY),
        });
        assert_eq!(skips(&events), 1);
    }

    #[test]
    fn requests_never_exceed_the_bound() {
        let events = run(|_| sample(FEATURE));
        assert!(events.iter().all(|e| e.0 <= WINDOW_MS));
        assert!(events.len() <= (WINDOW_MS / RETRY_EVERY_MS) as usize + 2);
    }

    #[test]
    fn the_grid_does_not_burst_after_a_long_confirmation() {
        // A confirmation that hid several grid ticks must not make the next
        // poll send one request per missed tick.
        let mut p = Progress::default();
        let menu = Sample {
            at_menu: true,
            ..sample(TitleKind::Menu)
        };
        assert_eq!(decide(&mut p, 1_900, menu), Decision::Wait);
        assert_eq!(decide(&mut p, 2_100, menu), Decision::Wait);
        assert_eq!(decide(&mut p, 2_300, sample(FEATURE)), Decision::RetryMenu);
        assert_eq!(decide(&mut p, 2_500, sample(FEATURE)), Decision::Wait);
        assert_eq!(decide(&mut p, 3_900, sample(FEATURE)), Decision::Wait);
        assert_eq!(decide(&mut p, 4_000, sample(FEATURE)), Decision::RetryMenu);
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
