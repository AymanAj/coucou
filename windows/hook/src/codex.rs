//! Windows Codex activity adapter. Permission decisions always stay in Codex.
use serde_json::{json, Value};

pub fn adapt(payload: &mut Value) -> Option<String> {
    let map = payload.as_object_mut()?;
    let event = map.get("hook_event_name")?.as_str()?.to_string();
    // Require the real session identity; never guess from cwd or terminal PID.
    let session = map.get("session_id")?.as_str()?;
    if session.is_empty() || session.len() > 256 || session.chars().any(char::is_control) {
        return None;
    }
    if !matches!(
        event.as_str(),
        "SessionStart"
            | "UserPromptSubmit"
            | "PreToolUse"
            | "PostToolUse"
            | "Stop"
            | "Interrupt"
            | "SessionEnd"
    ) {
        return None;
    }
    // PermissionRequest runs before automatic review in Codex. Installing it
    // would risk showing requests that Codex normally handles itself.
    // Only explicit structured failures are evidence; never scrape output text.
    let failed = map.get("tool_response").is_some_and(|v| {
        v.get("isError").and_then(Value::as_bool) == Some(true)
            || v.get("is_error").and_then(Value::as_bool) == Some(true)
            || v.get("exit_code")
                .and_then(Value::as_i64)
                .is_some_and(|code| code != 0)
    });
    map.insert("coucou_tool_failed".into(), json!(failed));
    map.remove("request_id");
    map.insert("coucou_agent".into(), json!("codex"));
    map.insert("coucou_codex_windows".into(), json!(true));
    // Drop all unneeded model/tool output rather than interpreting unstable text.
    map.remove("last_assistant_message");
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_documented_events_with_identity() {
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Stop",
            "Interrupt",
            "SessionEnd",
        ] {
            let mut v =
                json!({"hook_event_name":event,"session_id":"a","cwd":"C:\\a b","turn_id":"t"});
            assert_eq!(adapt(&mut v), Some(event.into()));
            assert_eq!(v["coucou_agent"], "codex");
            assert_eq!(v["turn_id"], "t");
        }
    }
    #[test]
    fn rejects_approvals_unknown_and_malformed_events() {
        for mut v in [
            json!(null),
            json!({}),
            json!({"hook_event_name":"Stop","session_id":42}),
            json!({"hook_event_name":"Stop","session_id":""}),
            json!({"hook_event_name":"PermissionRequest","session_id":"a"}),
            json!({"hook_event_name":"StopFailure","session_id":"a"}),
        ] {
            assert!(adapt(&mut v).is_none());
        }
    }
    #[test]
    fn only_explicit_structured_tool_failure_is_reported() {
        for (response, expected) in [
            (json!({"isError":true}), true),
            (json!({"exit_code":2}), true),
            (json!({"exit_code":0}), false),
            (json!("error message in normal output"), false),
        ] {
            let mut v =
                json!({"hook_event_name":"PostToolUse","session_id":"a","tool_response":response});
            adapt(&mut v).unwrap();
            assert_eq!(v["coucou_tool_failed"], expected);
        }
    }
    #[test]
    fn same_project_sessions_keep_distinct_ids() {
        let mut a = json!({"hook_event_name":"PreToolUse","session_id":"a","cwd":"C:/same"});
        let mut b = json!({"hook_event_name":"PreToolUse","session_id":"b","cwd":"C:/same"});
        adapt(&mut a);
        adapt(&mut b);
        assert_ne!(a["session_id"], b["session_id"]);
    }
}
