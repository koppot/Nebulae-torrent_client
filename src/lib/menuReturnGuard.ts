/**
 * Guards against popup-only / JumpTitle-1 Blu-rays where requesting Top Menu
 * restarts the active feature at 0:00 instead of opening an authored Top Menu.
 */

export interface MenuReturnGuardState {
  title: number;
  timeMs: number;
  deadline: number;
}

export interface MenuReturnGuardPoll {
  at_menu: boolean;
  playing: boolean;
  length_ms: number;
  title: number;
  time_ms: number;
}

export type MenuReturnGuardEvaluation =
  | { action: 'none' }
  | { action: 'disarm_menu_confirmed' }
  | { action: 'disarm_expired' }
  | { action: 'disarm_title_changed'; previousTitle: number; newTitle: number }
  | { action: 'recover_restart'; restoreTimeMs: number; mode: number };

/**
 * Arms the menu return guard when the user triggers Menu / M key.
 * Only arms if currently in a valid feature title (not already at menu).
 */
export function armMenuReturnGuard(
  poll: MenuReturnGuardPoll,
  now: number,
  timeoutMs: number = 5000,
): MenuReturnGuardState | null {
  if (poll.at_menu) {
    return null;
  }
  if (poll.length_ms <= 300_000 || poll.time_ms < 10_000) {
    return null;
  }
  return {
    title: poll.title,
    timeMs: poll.time_ms,
    deadline: now + timeoutMs,
  };
}

/**
 * Evaluates whether an armed guard should trigger recovery, disarm, or stay active.
 */
export function evaluateMenuReturnGuard(
  guard: MenuReturnGuardState | null,
  poll: MenuReturnGuardPoll,
  now: number,
  menuConfirmPolls: number,
): { nextGuard: MenuReturnGuardState | null; result: MenuReturnGuardEvaluation } {
  if (!guard) {
    return { nextGuard: null, result: { action: 'none' } };
  }

  // Real interactive menu confirmed open (>= 2 consecutive polls).
  if (menuConfirmPolls >= 2) {
    return { nextGuard: null, result: { action: 'disarm_menu_confirmed' } };
  }

  // Timeout reached without restart.
  if (now > guard.deadline) {
    return { nextGuard: null, result: { action: 'disarm_expired' } };
  }

  // Transition to a different title (e.g. bonus feature, different episode, or menu change).
  if (poll.title !== guard.title) {
    return {
      nextGuard: null,
      result: {
        action: 'disarm_title_changed',
        previousTitle: guard.title,
        newTitle: poll.title,
      },
    };
  }

  // Feature was restarted from 0:00 after pressing Menu on a popup-only disc.
  if (
    !poll.at_menu &&
    poll.playing &&
    poll.length_ms > 300_000 &&
    poll.title === guard.title &&
    poll.time_ms >= 0 &&
    poll.time_ms < 5_000 &&
    guard.timeMs >= 10_000
  ) {
    return {
      nextGuard: null,
      result: {
        action: 'recover_restart',
        restoreTimeMs: guard.timeMs,
        mode: 5, // BD_VK_POPUP
      },
    };
  }

  return { nextGuard: guard, result: { action: 'none' } };
}
