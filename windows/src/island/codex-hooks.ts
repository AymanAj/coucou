import { State } from "../core/state";
import { Sound } from "../core/sound";
import type { Island } from "./island";
import { CodexSessions, codexTaskId, parseCodexEvent } from "./codex";

const sessions = new CodexSessions();
export function handleCodexHook(island: Island, raw: unknown) {
  const event = parseCodexEvent(raw);
  if (!event || State.paused) return;
  const revision = sessions.accept(event);
  if (revision === null) return;
  const id = codexTaskId(event.session_id);
  if (event.hook_event_name === "SessionEnd") {
    if (State.tasks.find(t => t.id === id)?.state === "finished") return;
    State.removeTask(id);
    return;
  }
  State.upsertExternalAgent(id, "Codex", "#60A5FA");
  const task = State.tasks.find(t => t.id === id)!;
  task.source = "codex";
  task.sessionCwd = event.cwd ?? task.sessionCwd;
  // Reuse the existing project-name/ticker and pill layout.
  const project = task.sessionCwd?.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
  task.name = project ? `Codex · ${project}` : "Codex";
  if (State.mode !== "expanded" && !State.pendingApproval &&
      (State.focusTask?.state === "idle" || State.focusTask?.state === "finished") &&
      ["SessionStart", "UserPromptSubmit", "PreToolUse"].includes(event.hook_event_name)) {
    State.setFocus(id);
  }
  if (event.hook_event_name === "SessionStart" && event.source !== "compact") State.updateTask(id, "idle");
  if (event.hook_event_name === "UserPromptSubmit") {
    State.updateTask(id, "thinking");
    if (event.prompt) State.appendStep(id, event.prompt.slice(0, 60));
  }
  if (event.hook_event_name === "PreToolUse") {
    State.updateTask(id, "working");
    const tool = event.tool_name ?? "Tool";
    const label = tool === "Bash" ? "Exécute" : tool === "apply_patch" ? "Modifie" : tool;
    const command = event.tool_input?.command;
    State.appendStep(id, typeof command === "string" ? `${label} · ${command.slice(0, 40)}` : label);
  }
  if (event.hook_event_name === "PostToolUse") {
    State.updateTask(id, event.coucou_tool_failed ? "error" : "working");
    if (event.coucou_tool_failed) {
      State.appendStep(id, "Tool failed");
      State.setPillBadge(id, "error");
      Sound.play("error");
    }
  }
  if (event.hook_event_name === "Stop" || event.hook_event_name === "Interrupt") {
    const stopped = event.hook_event_name === "Interrupt";
    State.updateTask(id, stopped ? "idle" : "finished");
    if (stopped) State.appendStep(id, "Interrupted");
    else {
      Sound.play("finish");
      if (State.focusId === id) island.alert("finished");
      else State.setPillBadge(id, "finished");
    }
    window.setTimeout(() => {
      if (sessions.isCurrent(event.session_id, revision)) {
        const resetFinishedView = State.focusId === id && State.view === "finished";
        State.removeTask(id);
        if (resetFinishedView) island.setView(State.defaultView());
      }
    }, 5200);
  } else {
    if (!event.coucou_tool_failed) State.setPillBadge(id, null);
    if (event.hook_event_name === "SessionStart") Sound.play("work");
    if (State.mode === "hidden") island.reveal();
  }
  State.notify();
}
