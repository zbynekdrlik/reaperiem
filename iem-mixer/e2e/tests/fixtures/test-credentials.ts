/**
 * Centralized test credentials for live REAPER E2E tests.
 *
 * These PINs are configured on the production iem.lan REAPER project for
 * automated testing.  The set is intentionally small — only members whose
 * PINs are stable and documented (per project memory) are listed here.
 *
 * If a member's PIN changes (or a new test member is provisioned), update
 * this file rather than hardcoding the new value in individual specs.
 */

export const ENGINEER_PIN = "1177";

/**
 * Member PINs known to be stable on the production project.  The cross-member
 * isolation test only covers pairs whose PINs are listed here, because the
 * live test must hold an authenticated WebSocket open to populate
 * `mixer_cache.member_states[member]` before snapshot creation.
 */
export const KNOWN_MEMBER_PINS: Record<string, string> = {
  petronela: "7711",
  stevo: "7711",
};

/** Convenience pairs for cross-member isolation regression tests. */
export const ISOLATION_PAIRS: Array<[string, string, string]> = [
  ["petronela", "stevo", KNOWN_MEMBER_PINS.stevo],
  ["stevo", "petronela", KNOWN_MEMBER_PINS.petronela],
];
