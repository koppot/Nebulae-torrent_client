//! Decision logic that keeps a DVD "Play Main Feature" session on one title.
//!
//! The button reopens the disc with `:no-dvdnav-menu`. That only skips VLC's own
//! jump to the root menu: libdvdnav still starts at First Play and then follows
//! the disc's authored chain. On Banner of the Stars III (DISC_1) that meant a
//! clip (title 3) before the 30 minute episode (title 1) and the next title
//! (title 2) after it. The owner's rule for the button is that it plays only the
//! main episode, with nothing before it and nothing after it.
//!
//! Blu-ray is not affected: `:no-bluray-menu` makes libbluray play the main
//! playlist alone. Plain "Play" from the authored menu is not affected either;
//! the authored DVD menu keeps its own meaning. The automatic direct-playback
//! fallback of the menu watcher does not use the lock: only the explicit button
//! does.
//!
//! The watcher thread in `commands::media` owns every side effect (libVLC calls,
//! session checks). This module only decides, so the rules can be tested without
//! a player.

use super::engine::{TitleSnapshot, INPUT_TITLE_MENU};

/// A title shorter than this is a clip, not the main feature. It matches the
/// "feature title" rule of `player_step_title`.
pub const MIN_FEATURE_MS: i64 = 120_000;
/// Poll cadence while the lock is close to the end of its title or has not seen
/// the title yet.
pub const POLL_MS: u64 = 40;
/// Poll cadence while the title is far from its end. The media clock cannot
/// cross [`NEAR_END_MS`] in one interval even at the fastest playback rate.
pub const FAR_POLL_MS: u64 = 250;
/// Closer than this to the end of the title the lock polls at [`POLL_MS`].
pub const NEAR_END_MS: i64 = 10_000;
/// Poll cadence once the lock has paused at the end of its title and only waits
/// for a replay.
pub const IDLE_POLL_MS: u64 = 250;
/// How long the watcher waits for libVLC to publish the title list.
pub const TITLES_READY_TIMEOUT_MS: u64 = 5_000;
/// How long libVLC may take to report the title the lock asked for before the
/// lock gives up and leaves the disc's own flow alone.
pub const ENTER_TIMEOUT_MS: u64 = 4_000;
/// Pause this close to the end of the title. dvdnav reports the time once per
/// VOBU (up to about 1.2 s), so a smaller margin could be skipped over by the
/// disc's own jump to the next title.
pub const END_MARGIN_MS: i64 = 1_500;
/// After the pause at the end, playing again from this far before the end
/// (Replay, or a seek back) locks the title again.
pub const REARM_MARGIN_MS: i64 = 5_000;
/// A different title must show up in this many consecutive samples before it
/// counts as the disc moving on; the title index can flicker at a VTS change.
pub const FOREIGN_SAMPLES: u32 = 2;

/// VLC's dvdnav demuxer names the first entry of its title list "DVD Menu"
/// (3.0.x modules/access/dvdnav.c, `DemuxTitles`); libbluray calls its menu
/// "Top Menu". That is what tells the two disc kinds apart once a session runs.
const DVDNAV_MENU_NAME: &str = "DVD Menu";

/// Is this title list the one VLC's dvdnav demuxer builds?
pub fn is_dvdnav_title_list(titles: &[TitleSnapshot]) -> bool {
    titles.first().is_some_and(|t| {
        t.flags & INPUT_TITLE_MENU != 0 && t.name.as_deref() == Some(DVDNAV_MENU_NAME)
    })
}

/// The title "Play Main Feature" plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainTitle {
    /// libVLC title index (0 is the DVD menu, titles start at 1).
    pub index: i32,
    pub duration_ms: i64,
}

/// The first non-menu title of at least [`MIN_FEATURE_MS`]. Never the longest
/// title: on an episodic disc that is Play All, and on a movie disc it can be a
/// seamless-branch duplicate. `None` when every title is a clip; the session is
/// then left alone.
pub fn pick_main_title(titles: &[TitleSnapshot]) -> Option<MainTitle> {
    titles.iter().enumerate().find_map(|(i, t)| {
        let is_menu = t.flags & INPUT_TITLE_MENU != 0;
        (!is_menu && t.duration_ms >= MIN_FEATURE_MS).then_some(MainTitle {
            index: i as i32,
            duration_ms: t.duration_ms,
        })
    })
}

/// The title the viewer asked for with the step-title control while a lock is
/// active. A feature title is followed; the menu or a clip ends the lock, since
/// the viewer left the main title on purpose.
pub fn follow_request(titles: &[TitleSnapshot], requested: i32) -> Option<MainTitle> {
    let index = usize::try_from(requested).ok()?;
    let t = titles.get(index)?;
    let is_menu = t.flags & INPUT_TITLE_MENU != 0;
    (!is_menu && t.duration_ms >= MIN_FEATURE_MS).then_some(MainTitle {
        index: requested,
        duration_ms: t.duration_ms,
    })
}

/// The title the lock holds for a session that opened on title `current`.
///
/// A First Play chain that already landed on a feature-length title is left
/// on it: that is the disc's own choice of main feature, and jumping to the
/// first long title instead could pick a trailer reel on a movie disc. When
/// the chain starts on the menu, on a clip or on nothing yet, the lock moves
/// to the first feature title. `None` when the disc has no feature title.
pub fn choose_target(titles: &[TitleSnapshot], current: i32) -> Option<MainTitle> {
    follow_request(titles, current).or_else(|| pick_main_title(titles))
}

/// Is libVLC title `index` the authored menu? The dvdnav title list is fixed
/// when the disc opens, so the list the watcher copied at the start answers
/// this without another player call.
pub fn is_menu_index(titles: &[TitleSnapshot], index: i32) -> bool {
    usize::try_from(index)
        .ok()
        .and_then(|i| titles.get(i))
        .is_some_and(|t| t.flags & INPUT_TITLE_MENU != 0)
}

/// What the watcher sees at one poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// libVLC's current title index; -1 before the demuxer selected one.
    pub title: i32,
    /// Position in the current title; -1 when unknown.
    pub time_ms: i64,
    pub playing: bool,
    /// The current title is the authored menu.
    pub at_menu: bool,
}

/// Why the player should pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// The locked title has played to within [`END_MARGIN_MS`] of its end.
    EndOfTitle,
    /// The disc jumped to another title before the end was seen.
    DiscMovedOn,
}

/// Why the lock lets go of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseReason {
    /// The disc went back to its authored menu after the feature.
    BackAtMenu,
    /// libVLC never reported the locked title (the disc refused the jump).
    NeverEntered,
}

/// What the watcher thread should do at this poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Wait,
    /// Pause now. The lock stays armed to catch a replay.
    Pause(PauseReason),
    /// Drop the lock; the disc's own flow takes over.
    Release(ReleaseReason),
}

/// State of one locked title.
#[derive(Debug, Clone)]
pub struct Lock {
    title: i32,
    end_ms: i64,
    armed_at_ms: u64,
    entered: bool,
    foreign_streak: u32,
    finished: bool,
    last_time_ms: i64,
}

impl Lock {
    pub fn new(title: i32, end_ms: i64, now_ms: u64) -> Self {
        Self {
            title,
            end_ms,
            armed_at_ms: now_ms,
            entered: false,
            foreign_streak: 0,
            finished: false,
            last_time_ms: -1,
        }
    }

    /// The viewer stepped to another feature title: follow them.
    pub fn retarget(&mut self, title: i32, end_ms: i64, now_ms: u64) {
        *self = Self::new(title, end_ms, now_ms);
    }

    /// The locked title's index.
    pub fn title(&self) -> i32 {
        self.title
    }

    /// How long to sleep before the next poll.
    pub fn next_poll_ms(&self) -> u64 {
        if self.finished {
            IDLE_POLL_MS
        } else if self.entered
            && self.last_time_ms >= 0
            && self.end_ms - self.last_time_ms > NEAR_END_MS
        {
            FAR_POLL_MS
        } else {
            POLL_MS
        }
    }

    pub fn observe(&mut self, now_ms: u64, s: Sample) -> Step {
        let on_title = s.title == self.title;
        if on_title && s.time_ms >= 0 {
            self.last_time_ms = s.time_ms;
        }

        if self.finished {
            if on_title && s.playing && s.time_ms >= 0 && s.time_ms < self.end_ms - REARM_MARGIN_MS
            {
                self.finished = false;
                self.entered = true;
                self.foreign_streak = 0;
            }
            return Step::Wait;
        }

        if on_title {
            self.entered = true;
            self.foreign_streak = 0;
            if s.playing && s.time_ms >= self.end_ms - END_MARGIN_MS {
                self.finished = true;
                return Step::Pause(PauseReason::EndOfTitle);
            }
            return Step::Wait;
        }

        if s.title < 0 {
            return Step::Wait;
        }

        if !self.entered {
            if now_ms.saturating_sub(self.armed_at_ms) >= ENTER_TIMEOUT_MS {
                self.finished = true;
                return Step::Release(ReleaseReason::NeverEntered);
            }
            return Step::Wait;
        }

        self.foreign_streak += 1;
        if self.foreign_streak < FOREIGN_SAMPLES {
            return Step::Wait;
        }
        self.finished = true;
        if s.at_menu {
            Step::Release(ReleaseReason::BackAtMenu)
        } else {
            Step::Pause(PauseReason::DiscMovedOn)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(duration_ms: i64, flags: u32, name: Option<&str>) -> TitleSnapshot {
        TitleSnapshot {
            duration_ms,
            flags,
            name: name.map(str::to_string),
        }
    }

    /// DISC_1 of Banner of the Stars III as VLC lists it (B3 log, 2026-10-28):
    /// the menu, the 30:08 episode, a 3:46 extra and a clip.
    fn banner_titles() -> Vec<TitleSnapshot> {
        vec![
            snap(0, 0x03, Some("DVD Menu")),
            snap(1_808_266, 0x00, None),
            snap(225_666, 0x00, None),
            snap(28_933, 0x00, None),
        ]
    }

    const EPISODE_MS: i64 = 1_808_266;

    fn on(title: i32, time_ms: i64) -> Sample {
        Sample {
            title,
            time_ms,
            playing: true,
            at_menu: title == 0,
        }
    }

    fn banner_lock() -> Lock {
        Lock::new(1, EPISODE_MS, 0)
    }

    // ── title list ────────────────────────────────────────────────────────

    #[test]
    fn dvdnav_list_is_recognised_by_its_menu_entry() {
        assert!(is_dvdnav_title_list(&banner_titles()));
    }

    #[test]
    fn blu_ray_list_is_not_a_dvdnav_list() {
        let bd = vec![
            snap(0, 0x03, Some("Top Menu")),
            snap(5_400_000, 0x02, None),
            snap(0, 0x02, Some("First Play")),
        ];
        assert!(!is_dvdnav_title_list(&bd));
        assert!(!is_dvdnav_title_list(&[]));
    }

    #[test]
    fn a_title_named_dvd_menu_without_the_menu_flag_does_not_count() {
        let odd = vec![snap(0, 0x00, Some("DVD Menu")), snap(600_000, 0, None)];
        assert!(!is_dvdnav_title_list(&odd));
    }

    #[test]
    fn main_title_of_banner_is_the_episode_not_the_clip() {
        let main = pick_main_title(&banner_titles()).expect("main title");
        assert_eq!(main.index, 1);
        assert_eq!(main.duration_ms, EPISODE_MS);
    }

    #[test]
    fn main_title_is_the_first_feature_not_the_longest() {
        let mut titles = banner_titles();
        titles.push(snap(7_200_000, 0x00, None));
        assert_eq!(pick_main_title(&titles).map(|m| m.index), Some(1));
    }

    #[test]
    fn menu_flagged_titles_are_never_the_main_title() {
        let titles = vec![
            snap(600_000, 0x03, Some("DVD Menu")),
            snap(300_000, 0, None),
        ];
        assert_eq!(pick_main_title(&titles).map(|m| m.index), Some(1));
    }

    #[test]
    fn a_disc_of_clips_has_no_main_title() {
        let titles = vec![
            snap(0, 0x03, Some("DVD Menu")),
            snap(90_000, 0, None),
            snap(119_999, 0, None),
        ];
        assert_eq!(pick_main_title(&titles), None);
    }

    #[test]
    fn an_empty_list_has_no_main_title() {
        assert_eq!(pick_main_title(&[]), None);
    }

    // ── lock ──────────────────────────────────────────────────────────────

    #[test]
    fn waits_while_the_disc_is_still_on_the_clip() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(3, 500)), Step::Wait);
        assert_eq!(lock.observe(80, on(3, 540)), Step::Wait);
        assert_eq!(lock.observe(120, on(-1, -1)), Step::Wait);
    }

    #[test]
    fn plays_the_episode_without_interference() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(lock.observe(80, on(1, 653_632)), Step::Wait);
        assert_eq!(
            lock.observe(120, on(1, EPISODE_MS - END_MARGIN_MS - 1)),
            Step::Wait
        );
    }

    #[test]
    fn pauses_once_at_the_end_of_the_episode() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - END_MARGIN_MS)),
            Step::Pause(PauseReason::EndOfTitle)
        );
        // The disc's own jump to the next title after the pause is ignored.
        assert_eq!(lock.observe(120, on(1, EPISODE_MS - 100)), Step::Wait);
        assert_eq!(lock.observe(160, on(2, 0)), Step::Wait);
        assert_eq!(lock.observe(200, on(2, 40)), Step::Wait);
    }

    #[test]
    fn a_viewer_pause_near_the_end_needs_no_pause_from_the_lock() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        let paused = Sample {
            playing: false,
            ..on(1, EPISODE_MS - 200)
        };
        assert_eq!(lock.observe(80, paused), Step::Wait);
        // Playing again, the lock pauses at the end.
        assert_eq!(
            lock.observe(120, on(1, EPISODE_MS - 100)),
            Step::Pause(PauseReason::EndOfTitle)
        );
    }

    #[test]
    fn pauses_when_the_disc_moves_to_the_next_title_before_the_end_was_seen() {
        // dvdnav can end a title earlier than its nominal length.
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 1_700_000)), Step::Wait);
        assert_eq!(lock.observe(80, on(2, 0)), Step::Wait);
        assert_eq!(
            lock.observe(120, on(2, 40)),
            Step::Pause(PauseReason::DiscMovedOn)
        );
        assert_eq!(lock.observe(160, on(2, 40)), Step::Wait);
    }

    #[test]
    fn one_stray_sample_is_not_the_disc_moving_on() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 900_000)), Step::Wait);
        assert_eq!(lock.observe(80, on(0, 0)), Step::Wait);
        assert_eq!(lock.observe(120, on(1, 900_040)), Step::Wait);
        // The streak starts over: one more stray sample still is not enough.
        assert_eq!(lock.observe(160, on(2, 0)), Step::Wait);
        assert_eq!(lock.observe(200, on(1, 900_080)), Step::Wait);
    }

    #[test]
    fn going_back_to_the_menu_after_the_feature_releases_the_lock_without_pausing() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 1_700_000)), Step::Wait);
        assert_eq!(lock.observe(80, on(0, 0)), Step::Wait);
        assert_eq!(
            lock.observe(120, on(0, 0)),
            Step::Release(ReleaseReason::BackAtMenu)
        );
    }

    #[test]
    fn gives_up_when_libvlc_never_reports_the_locked_title() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(1_000, on(3, 1_000)), Step::Wait);
        assert_eq!(lock.observe(ENTER_TIMEOUT_MS - 1, on(3, 3_999)), Step::Wait);
        assert_eq!(
            lock.observe(ENTER_TIMEOUT_MS, on(3, 4_000)),
            Step::Release(ReleaseReason::NeverEntered)
        );
        assert_eq!(lock.observe(ENTER_TIMEOUT_MS + 40, on(1, 0)), Step::Wait);
    }

    #[test]
    fn an_unknown_title_never_counts_either_way() {
        let mut lock = banner_lock();
        for t in 0..200u64 {
            assert_eq!(lock.observe(t * 40, on(-1, -1)), Step::Wait);
        }
        assert_eq!(lock.observe(8_000, on(1, 0)), Step::Wait);
        // Entered: unknown samples still do not count as the disc moving on.
        for t in 0..10u64 {
            assert_eq!(lock.observe(8_040 + t * 40, on(-1, -1)), Step::Wait);
        }
    }

    #[test]
    fn replay_after_the_pause_locks_the_title_again() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - 500)),
            Step::Pause(PauseReason::EndOfTitle)
        );
        // Paused at the end: nothing happens.
        let paused = Sample {
            playing: false,
            ..on(1, EPISODE_MS - 500)
        };
        assert_eq!(lock.observe(120, paused), Step::Wait);
        // Replay seeks to 0 and plays.
        assert_eq!(lock.observe(160, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(200, on(1, EPISODE_MS - 1_000)),
            Step::Pause(PauseReason::EndOfTitle)
        );
    }

    #[test]
    fn a_small_seek_back_near_the_end_does_not_lock_again() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - 500)),
            Step::Pause(PauseReason::EndOfTitle)
        );
        assert_eq!(lock.observe(120, on(1, EPISODE_MS - 2_000)), Step::Wait);
        // Still within the re-arm margin: a second pause would loop forever.
        assert_eq!(lock.observe(160, on(1, EPISODE_MS - 1_900)), Step::Wait);
    }

    #[test]
    fn stepping_to_another_title_moves_the_lock_with_the_viewer() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 100_000)), Step::Wait);
        lock.retarget(2, 225_666, 100);
        // The disc still reports the old title for a moment.
        assert_eq!(lock.observe(120, on(1, 100_040)), Step::Wait);
        assert_eq!(lock.observe(160, on(2, 0)), Step::Wait);
        assert_eq!(
            lock.observe(200, on(2, 225_666 - END_MARGIN_MS)),
            Step::Pause(PauseReason::EndOfTitle)
        );
    }

    #[test]
    fn retarget_forgets_a_finished_lock() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - 100)),
            Step::Pause(PauseReason::EndOfTitle)
        );
        assert_eq!(lock.next_poll_ms(), IDLE_POLL_MS);
        lock.retarget(2, 225_666, 200);
        assert_eq!(lock.next_poll_ms(), POLL_MS);
        assert_eq!(lock.observe(240, on(2, 0)), Step::Wait);
    }

    #[test]
    fn full_banner_run_pauses_exactly_once() {
        // First Play clip (title 3) → jump to the episode → episode → title 2.
        let mut lock = banner_lock();
        let mut pauses = 0;
        let mut releases = 0;
        let mut now = 0u64;
        let mut feed = |lock: &mut Lock, s: Sample| {
            now += 40;
            match lock.observe(now, s) {
                Step::Pause(_) => pauses += 1,
                Step::Release(_) => releases += 1,
                Step::Wait => {}
            }
        };
        for t in [0, 40, 80] {
            feed(&mut lock, on(3, t));
        }
        for t in (0..EPISODE_MS - END_MARGIN_MS + 1_000).step_by(30_000) {
            feed(&mut lock, on(1, t));
        }
        for t in [0, 40, 80, 120] {
            feed(&mut lock, on(2, t));
        }
        assert_eq!(pauses, 1);
        assert_eq!(releases, 0);
    }

    // ── viewer's own title change ─────────────────────────────────────────

    #[test]
    fn a_requested_feature_title_is_followed() {
        let main = follow_request(&banner_titles(), 2).expect("title 2 is a feature");
        assert_eq!(main.index, 2);
        assert_eq!(main.duration_ms, 225_666);
    }

    #[test]
    fn a_requested_menu_clip_or_unknown_title_ends_the_lock() {
        let titles = banner_titles();
        assert_eq!(follow_request(&titles, 0), None);
        assert_eq!(follow_request(&titles, 3), None);
        assert_eq!(follow_request(&titles, 4), None);
        assert_eq!(follow_request(&titles, -1), None);
        assert_eq!(follow_request(&[], 1), None);
    }

    #[test]
    fn a_first_play_clip_is_replaced_by_the_first_feature() {
        // Banner: First Play starts on title 3, the 12.5 s clip.
        let target = choose_target(&banner_titles(), 3).expect("target");
        assert_eq!(target.index, 1);
        assert_eq!(target.duration_ms, EPISODE_MS);
    }

    #[test]
    fn a_menu_or_unknown_start_goes_to_the_first_feature() {
        assert_eq!(choose_target(&banner_titles(), 0).map(|t| t.index), Some(1));
        assert_eq!(
            choose_target(&banner_titles(), -1).map(|t| t.index),
            Some(1)
        );
    }

    #[test]
    fn a_first_play_that_lands_on_a_feature_stays_there() {
        // A movie disc: title 1 is a 2:30 trailer reel, the film is title 2.
        let movie = vec![
            snap(0, 0x03, Some("DVD Menu")),
            snap(150_000, 0, None),
            snap(7_200_000, 0, None),
        ];
        assert_eq!(choose_target(&movie, 2).map(|t| t.index), Some(2));
        assert_eq!(choose_target(&movie, 0).map(|t| t.index), Some(1));
    }

    #[test]
    fn no_target_on_a_disc_without_a_feature() {
        let clips = vec![snap(0, 0x03, Some("DVD Menu")), snap(30_000, 0, None)];
        assert_eq!(choose_target(&clips, 1), None);
    }

    #[test]
    fn menu_index_comes_from_the_copied_title_list() {
        let titles = banner_titles();
        assert!(is_menu_index(&titles, 0));
        assert!(!is_menu_index(&titles, 1));
        assert!(!is_menu_index(&titles, 9));
        assert!(!is_menu_index(&titles, -1));
    }

    // ── poll cadence ──────────────────────────────────────────────────────

    #[test]
    fn polls_fast_until_the_title_is_seen() {
        let lock = banner_lock();
        assert_eq!(lock.next_poll_ms(), POLL_MS);
    }

    #[test]
    fn polls_slowly_far_from_the_end_and_fast_near_it() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(lock.next_poll_ms(), FAR_POLL_MS);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - NEAR_END_MS - 1)),
            Step::Wait
        );
        assert_eq!(lock.next_poll_ms(), FAR_POLL_MS);
        assert_eq!(
            lock.observe(120, on(1, EPISODE_MS - NEAR_END_MS)),
            Step::Wait
        );
        assert_eq!(lock.next_poll_ms(), POLL_MS);
    }

    #[test]
    fn a_seek_toward_the_end_speeds_the_poll_up_again() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(lock.next_poll_ms(), FAR_POLL_MS);
        assert_eq!(lock.observe(290, on(1, EPISODE_MS - 4_000)), Step::Wait);
        assert_eq!(lock.next_poll_ms(), POLL_MS);
    }

    #[test]
    fn the_fast_poll_cannot_miss_the_end_at_the_fastest_rate() {
        // 32x playback: the clock moves 8 s per slow poll, less than NEAR_END_MS.
        assert!(FAR_POLL_MS as i64 * 32 < NEAR_END_MS);
    }

    #[test]
    fn idles_after_the_pause_and_polls_normally_after_a_replay() {
        let mut lock = banner_lock();
        assert_eq!(lock.observe(40, on(1, 0)), Step::Wait);
        assert_eq!(
            lock.observe(80, on(1, EPISODE_MS - 100)),
            Step::Pause(PauseReason::EndOfTitle)
        );
        assert_eq!(lock.next_poll_ms(), IDLE_POLL_MS);
        assert_eq!(lock.observe(330, on(1, 0)), Step::Wait);
        assert_eq!(lock.next_poll_ms(), FAR_POLL_MS);
    }
}
