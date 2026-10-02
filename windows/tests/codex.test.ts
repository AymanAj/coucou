import test from "node:test";
import assert from "node:assert/strict";
import { CODEX_EVENTS, CodexSessions, codexTaskId, parseCodexEvent } from "../src/island/codex";
import { handleCodexHook } from "../src/island/codex-hooks";
import { State, type AgentTask } from "../src/core/state";
import { Ticker } from "../src/views/ticker";
import type { Island } from "../src/island/island";

const event = (session = "a", name = "PreToolUse", turn = "t1") => ({
  coucou_agent: "codex", coucou_codex_windows: true, session_id: session, cwd: "C:\\same project", hook_event_name: name,
  turn_id: turn, tool_name: "Bash", tool_input: { command: "identical command" },
});
test("documented activity events retain session, cwd and tool context", () => {
  for (const name of CODEX_EVENTS) {
    const parsed = parseCodexEvent(event("real-id", name));
    assert.equal(parsed?.session_id, "real-id");
    assert.equal(parsed?.cwd, "C:\\same project");
    assert.equal(parsed?.hook_event_name, name);
  }
});
test("malformed, unknown, approval and Claude payloads do not enter Codex state", () => {
  for (const raw of [null, [], 12, {}, event("", "Stop"), event("a", "FutureEvent"),
    event("a", "PermissionRequest"), { ...event(), coucou_agent: undefined },
    { ...event(), session_id: 12 }, event("bad\nid")]) assert.equal(parseCodexEvent(raw), null);
});
test("same cwd and identical commands remain isolated by full session id", () => {
  assert.notEqual(codexTaskId("a"), codexTaskId("b"));
  const sessions = new CodexSessions();
  const a = sessions.accept(parseCodexEvent(event("a"))!)!;
  const b = sessions.accept(parseCodexEvent(event("b"))!)!;
  assert.ok(sessions.isCurrent("a", a)); assert.ok(sessions.isCurrent("b", b));
});
test("old completion cannot remove a resumed session or a newer turn", () => {
  const sessions = new CodexSessions();
  const finished = sessions.accept(parseCodexEvent(event("a", "Stop"))!)!;
  sessions.accept(parseCodexEvent(event("a", "UserPromptSubmit", "t2"))!);
  assert.equal(sessions.isCurrent("a", finished), false);
  assert.equal(sessions.accept(parseCodexEvent(event("a", "Stop", "t1"))!), null);
  sessions.accept(parseCodexEvent(event("a", "SessionEnd", "t2"))!);
  assert.equal(sessions.accept(parseCodexEvent(event("a", "PostToolUse", "t2"))!), null);
  sessions.accept(parseCodexEvent(event("a", "SessionStart", "t3"))!);
  assert.equal(sessions.isCurrent("a", finished), false);
});
test("session end preserves completion until its existing timer expires", () => {
  const sessions = new CodexSessions();
  const completed = sessions.accept(parseCodexEvent(event("a", "Stop"))!)!;
  sessions.accept(parseCodexEvent(event("a", "SessionEnd"))!);
  assert.ok(sessions.isCurrent("a", completed));
  assert.equal(sessions.accept(parseCodexEvent(event("a", "PreToolUse"))!), null);
  sessions.accept(parseCodexEvent(event("a", "SessionStart", "t2"))!);
  assert.equal(sessions.isCurrent("a", completed), false);
});
test("ticker reset prevents same-index session text and queued animation leaking", () => {
  // Small recording DOM for the production ticker; native smoke covers rendering.
  class Element {
    style = {}; children: Element[] = []; ownText = "";
    setAttribute() {}
    append(child: Element) { this.children.push(child); }
    set textContent(value: string) { this.ownText = value; this.children = []; }
    get textContent(): string { return this.ownText + this.children.map(c => c.textContent).join(""); }
  }
  const previous = globalThis.document;
  Object.assign(globalThis, { document: {
    createElement: () => new Element(), createElementNS: () => new Element(),
    createTextNode: (text: string) => Object.assign(new Element(), { ownText: text }),
  } });
  try {
    const ticker = new Ticker();
    const task = (id: string, steps: string[]): AgentTask => ({
      id, name: "Codex", color: "#60A5FA", source: "codex", state: "working",
      steps, stepIndex: steps.length - 1, isIntegration: false,
    });
    ticker.sync(task("a", ["A prompt", "A edit"]));
    ticker.sync(task("a", ["A prompt", "A edit", "A queued command"]));
    ticker.tick(100);
    assert.ok(ticker.el.textContent?.includes("A queued command"));
    ticker.reset();
    ticker.sync(task("b", ["B prompt", "B edit", "B command"]));
    ticker.tick(1000); ticker.tick(2000);
    assert.ok(ticker.el.textContent?.includes("B command"));
    assert.ok(!ticker.el.textContent?.includes("A "));
    assert.equal(ticker.animating, false);
  } finally { Object.assign(globalThis, { document: previous }); }
});

test("existing island state receives Codex activity without changing Claude", () => {
  const timers: (() => void)[] = [];
  (globalThis as unknown as { window: unknown }).window = { setTimeout: (fn: () => void) => timers.push(fn) };
  const island = { reveal() {}, alert() {}, setView(view: typeof State.view) { State.view = view; } } as unknown as Island;
  State.tasks = []; State.focusId = null; State.loadIntegrationTasks();
  const claude = structuredClone(State.tasks.find(t => t.id === "integration_claude"));
  handleCodexHook(island, event("ui-a", "UserPromptSubmit"));
  handleCodexHook(island, event("ui-b", "PreToolUse"));
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-a"))?.state, "thinking");
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-b"))?.state, "working");
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-b"))?.sessionCwd, "C:\\same project");
  handleCodexHook(island, { ...event("ui-a", "SessionStart"), source: "compact" });
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-a"))?.state, "thinking");
  handleCodexHook(island, event("ui-a", "Stop"));
  handleCodexHook(island, event("ui-a", "UserPromptSubmit", "t2"));
  timers[0]();
  assert.ok(State.tasks.some(t => t.id === codexTaskId("ui-a")));
  handleCodexHook(island, event("ui-b", "Interrupt"));
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-b"))?.state, "idle");
  handleCodexHook(island, event("ui-b", "SessionEnd"));
  assert.ok(!State.tasks.some(t => t.id === codexTaskId("ui-b")));
  handleCodexHook(island, { ...event("ui-a", "PostToolUse", "t2"), coucou_tool_failed: true });
  assert.equal(State.tasks.find(t => t.id === codexTaskId("ui-a"))?.state, "error");
  handleCodexHook(island, event("ui-a", "PermissionRequest", "t2"));
  assert.equal(State.pendingApproval, null);
  assert.deepEqual(State.tasks.find(t => t.id === "integration_claude"), claude);
  handleCodexHook(island, event("ui-a", "Stop", "t2"));
  State.focusId = codexTaskId("ui-a"); State.view = "finished";
  timers.at(-1)!();
  assert.equal(State.view, "overview");
  assert.ok(!State.tasks.some(t => t.id === codexTaskId("ui-a")));
  State.paused = true;
  handleCodexHook(island, event("paused"));
  assert.ok(!State.tasks.some(t => t.id === codexTaskId("paused")));
  State.paused = false;
});
