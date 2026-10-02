# Codex sessions on Windows

Coucou observes an existing local Codex coding session. It uses no OpenAI API key,
does not start a separate chat, and does not replace Codex's client or approval
policy. The existing Windows island, Mochi, ticker, pills, sounds, completion card
and retraction remain the UI. Claude Code retains its separate existing pipeline.

## Architecture and supported activity

```
Codex command hooks
  -> coucou-hook.exe --agent codex (Windows adapter)
  -> existing same-user Windows named pipe
  -> existing `hook` event
  -> parallel Codex session handler
  -> existing Coucou state/views/Mochi
```

The adapter accepts documented JSON on stdin, validates the event and real
`session_id`, tags the Windows source, and forwards bounded activity metadata.
It preserves `cwd`, `turn_id` and tool context. It discards transcript paths,
assistant output, tool output and approval request identifiers. There is no
transcript/terminal scraping, input automation or process injection.

| Codex event | Existing Coucou behavior |
| --- | --- |
| `SessionStart` | Own session pill, project basename, idle; compact does not reset activity |
| `UserPromptSubmit` | Thinking, prompt ticker |
| `PreToolUse` | Working, tool/command ticker; `apply_patch` labelled editing |
| `PostToolUse` | Working; error only for explicit structured failure metadata |
| `Stop` | Turn completed, existing sound/card/badge, five-second cleanup |
| `Interrupt` | Idle, interrupted ticker, delayed cleanup |
| `SessionEnd` | Remove own pill; retain a completed card until its timer expires |

Each pill uses the **full session ID**, not the project path or command. Two
sessions in the same directory remain distinct. Turn/revision tracking prevents
a stale completion timer from deleting a resumed session and rejects older-turn
events. Compact/hidden idle islands can focus Codex; an expanded view, active
agent or Claude approval is not displaced by session detection.
The shared ticker is re-seeded on focus changes involving Codex, so equal step
counts or queued animations cannot show a different session's tool activity.

Tool names are shown as exposed by Codex. A shell command is not classified as
reading/editing by guessing its text. Hosted tools do not all have local tool
hooks. This phase does not invent waiting/question events, crash notifications,
or Claude's `StopFailure`/`PostToolUseFailure` equivalents. `Stop` means the turn
ended, not that the requested coding task succeeded. Failure is displayed only
when `PostToolUse.tool_response` explicitly contains `isError: true`,
`is_error: true`, or a numeric nonzero `exit_code`; opaque textual output is not
parsed. A hard crash may leave a pill until Coucou is restarted because Codex
cannot guarantee `SessionEnd` in that case.

## Why Allow/Deny remains in Codex

Codex does have a supported `PermissionRequest` hook with allow, deny and
no-decision responses. However, the current official implementation invokes it
**before** routing to Guardian/automatic review or a user prompt. Its documented
payload does not establish that automatic review has finished and a human
decision is actually needed. Showing every such hook as a Coucou approval would
ask about actions Codex otherwise handles automatically, violating this phase's
requirements.

Consequently **no `PermissionRequest` hook is installed**, and the Windows Codex
adapter rejects that event immediately without a verdict. No Codex approval card
or request-scoped Allow/Deny is implemented. Codex's normal policy, reviewer and
prompt remain authoritative; existing Claude approval behavior is unchanged.

Codex app-server has approval requests/notifications for sessions owned by its
client. It is not a supported attachment mechanism for observing arbitrary
already-running Codex CLI sessions. Replacing the user's client with an
app-server launcher would be a different phase, so it is not used here.

## Setup and removal

1. Start the Windows Coucou application. Its existing startup copies the relay
   into `%LOCALAPPDATA%\Coucou\bin\coucou-hook.exe`.
2. Open **Settings -> Codex -> Install hooks**. Review the owned-entry summary
   and confirm using the same existing installer UI as Claude Code.
3. Review and explicitly trust the exact new definitions in Codex **`/hooks`**,
   then start a new local Windows Codex session. Changing definitions can require
   renewed trust. Coucou never grants trust or changes approval/security policy.

The file is `%CODEX_HOME%\hooks.json` when Coucou has `CODEX_HOME`, otherwise
`%USERPROFILE%\.codex\hooks.json`. Run Coucou and Codex with the same Codex home.
User-level `config.toml` is inspected but never written. Existing inline hooks,
managed policy, feature flags and trust remain in force. If the inspected config
disables hooks, the preview reports it and preserves that choice. Additional
project/profile/managed restrictions can also prevent hooks from running.

Installation adds seven command handlers with
`statusMessage: "Coucou Codex activity (Windows)"`. `commandWindows` invokes
PowerShell with a literal, UTF-16LE encoded command for the relay path and
`--agent codex`. This avoids shell/path quoting ambiguities. The non-Windows
command is `exit 0`; this integration is Windows-only.

Existing JSON fields, foreign event groups and foreign handlers are preserved.
A dated, unique `hooks.json.coucou-backup-*` stores the original bytes before
each real replacement. Invalid/unreadable files, stale previews and reparse
points are refused. Replacement uses a synced temporary file and an atomic
rename. Reinstalling is idempotent. **Uninstall hooks** removes only handlers
with Coucou's ownership marker and command signature; repeated uninstall is a
no-op. It may leave harmless empty event arrays. `config.toml` and hook trust are
untouched in both directions. Backups may include private foreign commands;
keep them private with the existing Codex home.

Preview deliberately summarizes only Coucou-owned changes rather than exposing
somebody else's command contents. No setup was installed into the real user
Codex home during development; setup tests used isolated fixtures.

## Optional infrastructure and timeouts

Hooks are synchronous to preserve event order, but never wait for Coucou to make
a decision. The relay retains the existing 300 ms connection budget and two
second whole-operation deadline. It exits successfully with `{}` for accepted
Codex activity, including when Coucou is closed or an accepted pipe stops
reading. `{}` supplies no loop or approval decision. Installed hooks also have
a three-second Codex timeout, covering a slow shell/startup or relay crash.
Malformed/unsupported input exits successfully with no verdict. Coucou absence
can add bounded hook-launch overhead, but cannot leave a session waiting for a
human or indefinitely blocked by Coucou.

## Interfaces and versions researched

Research and Windows validation were performed on 2026-10-02. Installed npm CLI
was **0.146.0**; the installed Codex desktop engine was **0.159.2**. Current
official stable source **rust-v0.160.0** was also checked for approval ordering.
This is interface compatibility evidence, not a guarantee for every older CLI
or every remote execution environment.

This adapter requires **locally orchestrated Windows Codex sessions**. The
official hooks documentation excludes local command hooks from Work Cloud
orchestration, including cloud sessions whose tools execute locally. Those
sessions cannot be observed through this adapter; no cloud-hook integration is
claimed or added.

Official sources:

- [Codex hooks](https://learn.chatgpt.com/docs/hooks): common input, lifecycle,
  tool hooks, Windows commands, configuration, trust, timeouts and responses.
- [PermissionRequest payload and decisions](https://learn.chatgpt.com/docs/hooks#permissionrequest).
- [Official approval routing, rust-v0.160.0](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core/src/tools/approvals.rs):
  `request_approval` runs permission hooks before `request_reviewer_approval`;
  no hook verdict preserves normal review.
- [Official permission event schema, rust-v0.160.0](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/hooks/src/events/permission_request.rs).
- [Official Windows command runner, rust-v0.160.0](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/hooks/src/engine/command_runner.rs).
- [Codex app-server](https://developers.openai.com/codex/app-server): client-owned
  thread/turn/event and approval transport, not an existing CLI session tap.

## Verification

Run from `windows/` with the normal Rust/MSVC/Windows SDK and Node prerequisites:

```powershell
npm ci
cargo build --release -p coucou-hook
cargo test --workspace
cargo clippy --workspace --all-targets
npm test
npx tsc --noEmit
npm run tauri -- build --no-bundle
# Close Coucou before testing the real pipe; this refuses an occupied pipe.
python tests/relay-windows.py target/release/coucou-hook.exe
```

Actual development results:

- Native Windows release application and relay built successfully with Tauri;
  TypeScript checking and Vite production build passed. Installer packaging,
  signing and distribution were not run.
- Clippy completed successfully, with three warnings in unchanged existing
  `src-tauri/src/hooks.rs` and `src-tauri/src/island.rs`; no new adapter warnings.
- All **20 Rust tests** passed: 11 existing Windows/Claude tests and nine new
  Codex parsing/config tests. All **seven TypeScript tests** passed, including
  session isolation, stale-turn/timer protection, malformed/unknown events,
  pause/compaction, structured errors and unchanged Claude task state.
- Five actual Windows named-pipe/relay checks passed: absent app, forwarding,
  stalled write (about two seconds), rejected Codex permission, and unchanged
  Claude Allow response shape. These use a controlled pipe server, not a real
  Claude model session.
- The native Settings webview and actual Tauri commands passed isolated
  install/reinstall/uninstall, foreign hook preservation, backup and unchanged
  disabled `config.toml` checks.
- A real **0.159.2 Codex engine** turn reached the native Coucou island through
  invocation-only inline hook definitions: SessionStart/UserPromptSubmit/Stop/
  SessionEnd. The project/session identity and completion/reset were checked.
  Test-only hook trust bypass was restricted to that invocation and these vetted
  definitions; no persistent hook trust/security settings were changed.
- Native tool/error/cancellation/session concurrency activity was checked with
  controlled official-payload fixtures. A real 0.146.0 shell attempt was declined
  by the existing policy; a real tool-success/error or human approval round trip
  is **not** claimed. Actual normal user-level hook installation and `/hooks`
  trust still need a user's explicit review in their own session.

No lint script is configured in this Windows project. No macOS/Linux build,
new API chat, OpenAI key, terminal focusing, hosted-client attachment, or Codex
interactive Allow/Deny implementation was added or validated.

Next validation: install from Windows Settings, explicitly review `/hooks`, and
exercise normal interactive Codex start/resume, a permitted local tool, interrupt
and two concurrent sessions. Any future approval phase needs an official event
that distinguishes an actual pending human decision after automatic review.
