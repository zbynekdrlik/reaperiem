#!/bin/bash
# .claude/hooks/block-cargo.sh
# SAFETY: Blocks local cargo compilation - all builds run on GitHub CI only
# See CLAUDE.md: "### Build Commands (run on GitHub Actions, not locally)"

set -e

# Read input from stdin
INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

# Check if the command contains cargo build, cargo test, cargo check, or cargo clippy
if echo "$COMMAND" | grep -qE '(^|\s|&&|\|)(cargo\s+(build|test|check|clippy|run|bench))'; then
  # Extract the subcommand for a specific message
  SUBCOMMAND=$(echo "$COMMAND" | grep -oE 'cargo\s+(build|test|check|clippy|run|bench)' | head -1 | awk '{print $2}')

  case "$SUBCOMMAND" in
    build)
      REASON="LOCAL COMPILATION BLOCKED: 'cargo build' must run on GitHub CI. Push your changes: git add, git commit, git push, then 'gh run watch'"
      ;;
    test)
      REASON="LOCAL TESTS BLOCKED: 'cargo test' must run on GitHub CI with zero-tolerance enforcement. Push your changes: git add, git commit, git push, then 'gh run watch'"
      ;;
    check)
      REASON="LOCAL CHECK BLOCKED: 'cargo check' must run on GitHub CI. Push your changes: git add, git commit, git push, then 'gh run watch'"
      ;;
    clippy)
      REASON="LOCAL LINT BLOCKED: 'cargo clippy' must run on GitHub CI. Push your changes: git add, git commit, git push, then 'gh run watch'"
      ;;
    run)
      REASON="LOCAL RUN BLOCKED: 'cargo run' must run on GitHub CI or target. Push your changes and use deployment workflow."
      ;;
    bench)
      REASON="LOCAL BENCHMARK BLOCKED: 'cargo bench' must run on GitHub CI. Push your changes: git add, git commit, git push"
      ;;
    *)
      REASON="LOCAL CARGO COMMAND BLOCKED: Must run on GitHub CI, not locally. Push your changes."
      ;;
  esac

  # Deny the command
  jq -n --arg reason "$REASON" '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: $reason
    }
  }'
  exit 0
fi

# Allow all other commands (exit 0 with no output = allow)
exit 0
