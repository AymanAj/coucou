import test from "node:test";
import assert from "node:assert/strict";
import { buildFinished, type ViewActions } from "../src/views/views";
import { Bridge } from "../src/core/bridge";
import { State, type AgentTask, type AgentSource } from "../src/core/state";

// Recording DOM exercises the production finished view without a browser dependency.
class Element {
  style = { setProperty() {} }; children: Element[] = []; ownText = "";
  listeners = new Map<string, () => void>();
  constructor(readonly tag: string) {}
  setAttribute() {}
  addEventListener(name: string, listener: () => void) { this.listeners.set(name, listener); }
  append(child: Element) { this.children.push(child); }
  get firstChild() { return this.children[0]; }
  removeChild(child: Element) { this.children = this.children.filter(c => c !== child); }
  set textContent(value: string) { this.ownText = value; this.children = []; }
  get textContent(): string { return this.ownText + this.children.map(c => c.textContent).join(""); }
  querySelector(tag: string): Element | undefined {
    for (const child of this.children) {
      if (child.tag === tag) return child;
      const found = child.querySelector(tag); if (found) return found;
    }
  }
  click() { this.listeners.get("click")?.(); }
}
function fixture(run: (view: ReturnType<typeof buildFinished>, calls: string[], focus: (source: AgentSource, sessionId?: string) => void) => void) {
  const previous = { document: globalThis.document, tasks: State.tasks, focus: State.focusId,
    codex: Bridge.openCodex, vscode: Bridge.openInVSCode };
  Object.assign(globalThis, { document: {
    createElement: (tag: string) => new Element(tag), createElementNS: (_namespace: string, tag: string) => new Element(tag),
    createTextNode: (text: string) => Object.assign(new Element("#text"), { ownText: text }),
  } });
  const calls: string[] = [];
  Bridge.openCodex = async (sessionId) => { calls.push(`open_codex:${sessionId}`); return true; };
  Bridge.openInVSCode = async () => { calls.push("open_in_vscode"); return true; };
  const focus = (source: AgentSource, sessionId = "01a0fd14-c289-7712-9ffe-93df5a02a0fa") => {
    const task: AgentTask = { id: source === "codex" ? `agent_codex:${sessionId}` : source, name: source, source, color: "#60A5FA", state: "finished",
      steps: ["Done"], stepIndex: 0, isIntegration: false };
    State.tasks = [task]; State.focusId = task.id;
  };
  try {
    const actions = { openCodex: (sessionId: string) => { void Bridge.openCodex(sessionId); },
      openTerminal: () => { void Bridge.openInVSCode(null); }, collapse() {} } as ViewActions;
    run(buildFinished(actions), calls, focus);
  } finally {
    Object.assign(globalThis, { document: previous.document });
    State.tasks = previous.tasks; State.focusId = previous.focus;
    Bridge.openCodex = previous.codex; Bridge.openInVSCode = previous.vscode;
  }
}
test("Codex finished card says Open Codex", () => fixture((view, _calls, focus) => {
  focus("codex"); view.sync();
  assert.equal(view.el.querySelector("button")?.textContent, "Open Codex");
}));
test("Claude finished card keeps Open terminal", () => fixture((view, _calls, focus) => {
  focus("claudeCode"); view.sync();
  assert.equal(view.el.querySelector("button")?.textContent, "Open terminal");
}));
test("finished CTA uses Codex launcher exclusively and switches back to unchanged VS Code action", () => fixture((view, calls, focus) => {
  const click = () => (view.el.querySelector("button") as unknown as Element).click();
  focus("codex"); view.sync(); click();
  assert.deepEqual(calls, ["open_codex:01a0fd14-c289-7712-9ffe-93df5a02a0fa"]);
  focus("claudeCode"); view.sync(); click();
  assert.deepEqual(calls, ["open_codex:01a0fd14-c289-7712-9ffe-93df5a02a0fa", "open_in_vscode"]);
  assert.equal(view.el.querySelector("button")?.textContent, "Open terminal");
}));

test("Codex CTA preserves the full ID and isolates simultaneous session targets", () => fixture((view, calls, focus) => {
  const click = () => (view.el.querySelector("button") as unknown as Element).click();
  const first = "01a0fd14-c289-7712-9ffe-93df5a02a0fa";
  const second = "01a0fcef-9964-7901-ac17-47e321b6cab5";
  focus("codex", first); view.sync(); click();
  focus("codex", second); view.sync(); click();
  assert.deepEqual(calls, [`open_codex:${first}`, `open_codex:${second}`]);
}));
test("rendered Codex CTA cannot fall through to Claude when its task is removed", () => fixture((view, calls, focus) => {
  focus("codex"); view.sync();
  State.tasks = []; State.focusId = null;
  (view.el.querySelector("button") as unknown as Element).click();
  assert.deepEqual(calls, ["open_codex:01a0fd14-c289-7712-9ffe-93df5a02a0fa"]);
}));
