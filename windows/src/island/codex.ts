// Official Codex hook payloads only. No transcript parsing or approval control.
export const CODEX_EVENTS = [
  "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop", "Interrupt", "SessionEnd",
] as const;
export interface CodexEvent {
  coucou_agent: "codex";
  coucou_codex_windows: true;
  hook_event_name: typeof CODEX_EVENTS[number];
  session_id: string;
  turn_id?: string;
  cwd?: string;
  source?: string;
  prompt?: string;
  tool_name?: string;
  coucou_tool_failed?: boolean;
  tool_input?: Record<string, unknown>;
}
export function parseCodexEvent(raw: unknown): CodexEvent | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const p = raw as Record<string, unknown>;
  if (p.coucou_agent !== "codex" || p.coucou_codex_windows !== true || typeof p.session_id !== "string" ||
      !p.session_id || p.session_id.length > 256 || /[\x00-\x1f\x7f]/.test(p.session_id) ||
      !CODEX_EVENTS.includes(p.hook_event_name as CodexEvent["hook_event_name"])) return null;
  const event: CodexEvent = {
    coucou_agent: "codex", coucou_codex_windows: true, session_id: p.session_id,
    hook_event_name: p.hook_event_name as CodexEvent["hook_event_name"],
  };
  event.coucou_tool_failed = p.coucou_tool_failed === true;
  for (const key of ["turn_id", "cwd", "source", "prompt", "tool_name"] as const) {
    if (typeof p[key] === "string") event[key] = p[key];
  }
  if (p.tool_input && typeof p.tool_input === "object" && !Array.isArray(p.tool_input)) {
    event.tool_input = p.tool_input as Record<string, unknown>;
  }
  return event;
}
// Session ids are kept whole, including when two sessions have the same cwd.
export function codexTaskId(sessionId: string): string { return `agent_codex:${sessionId}`; }

/** Invalidates delayed completion removal as soon as the session works again. */
export class CodexSessions {
  private sessions = new Map<string, { revision: number; turn?: string; ended?: boolean; finished?: boolean }>();
  private revision = 0;
  accept(event: CodexEvent): number | null {
    const previous = this.sessions.get(event.session_id);
    if (previous?.ended && event.hook_event_name !== "SessionStart" && event.hook_event_name !== "UserPromptSubmit") return null;
    if (previous?.turn && event.turn_id && previous.turn !== event.turn_id &&
        event.hook_event_name !== "UserPromptSubmit" && event.hook_event_name !== "SessionStart") return null;
    if (event.hook_event_name === "SessionEnd" && previous?.finished) {
      this.sessions.set(event.session_id, { ...previous, ended: true });
      return previous.revision; // Preserve the existing five-second completion card.
    }
    const revision = ++this.revision;
    this.sessions.set(event.session_id, {
      revision, turn: event.turn_id ?? previous?.turn, ended: event.hook_event_name === "SessionEnd",
      finished: event.hook_event_name === "Stop",
    });
    return revision;
  }
  isCurrent(session: string, revision: number): boolean {
    return this.sessions.get(session)?.revision === revision;
  }
}
