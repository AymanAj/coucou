"""Real Windows named-pipe/relay checks. No real Codex/Claude config is touched.

Run while Coucou is closed: python tests/relay-windows.py target/release/coucou-hook.exe
The first-instance flag refuses to replace a running Coucou server.
"""
import ctypes
from ctypes import wintypes as w
import json
import subprocess
import sys
import threading
import time

kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.CreateNamedPipeW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, w.DWORD, w.DWORD, w.DWORD, w.DWORD, w.LPVOID]
kernel.CreateNamedPipeW.restype = w.HANDLE
kernel.ConnectNamedPipe.argtypes = [w.HANDLE, w.LPVOID]
kernel.ReadFile.argtypes = [w.HANDLE, w.LPVOID, w.DWORD, ctypes.POINTER(w.DWORD), w.LPVOID]
kernel.WriteFile.argtypes = [w.HANDLE, w.LPCVOID, w.DWORD, ctypes.POINTER(w.DWORD), w.LPVOID]
kernel.CloseHandle.argtypes = [w.HANDLE]
kernel.DisconnectNamedPipe.argtypes = [w.HANDLE]
sid = subprocess.check_output(["powershell.exe", "-NoProfile", "-Command", "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value"], text=True).strip()
pipe_name = "\\\\.\\pipe\\coucou-" + sid
exe = sys.argv[1]
payload = {"hook_event_name": "PreToolUse", "session_id": "relay-test", "cwd": "C:\\a b", "tool_name": "Bash", "tool_input": {"command": "echo test"}}

def run(data=payload, agent="codex"):
    started = time.monotonic()
    args = [exe] + (["--agent", agent] if agent else [])
    result = subprocess.run(args, input=json.dumps(data), text=True, capture_output=True, timeout=5)
    assert result.returncode == 0, result.stderr
    return result.stdout.strip(), time.monotonic() - started

def server(action):
    # Duplex byte pipe; first instance prevents collisions with the actual app.
    handle = kernel.CreateNamedPipeW(pipe_name, 3 | 0x80000, 0, 1, 64, 64, 0, None)
    if handle == w.HANDLE(-1).value:
        raise RuntimeError("Pipe unavailable; close Coucou before this isolated test")
    errors = []
    def serve():
        try:
            if not kernel.ConnectNamedPipe(handle, None) and ctypes.get_last_error() != 535:
                raise ctypes.WinError(ctypes.get_last_error())
            action(handle)
        except Exception as err:
            errors.append(err)
        finally:
            kernel.DisconnectNamedPipe(handle)
            kernel.CloseHandle(handle)
    thread = threading.Thread(target=serve)
    thread.start()
    return thread, errors

def read(handle):
    data = b""
    while b"\n" not in data:
        buf = ctypes.create_string_buffer(4096)
        n = w.DWORD()
        if not kernel.ReadFile(handle, buf, 4096, ctypes.byref(n), None):
            raise ctypes.WinError(ctypes.get_last_error())
        data += buf.raw[:n.value]
    return json.loads(data.split(b"\n")[0])

# Closed Coucou: both agents keep their original safe no-decision contract.
for agent in ["codex", ""]:
    output, elapsed = run(agent=agent)
    assert output == ("{}" if agent else ""), output
    assert elapsed < 1, elapsed
print("PASS absent Coucou: Codex and Claude exit cleanly under 1s")

received = []
thread, errors = server(lambda handle: received.append(read(handle)))
output, elapsed = run()
thread.join(5)
assert not thread.is_alive() and not errors, errors
assert received[0]["session_id"] == "relay-test" and received[0]["coucou_agent"] == "codex"
assert output == "{}"
print("PASS real named pipe forwards session/cwd/tool context")

# A same-user server accepts but never reads. The relay's own deadline wins.
thread, errors = server(lambda _: time.sleep(2.5))
large = {**payload, "tool_input": {str(i): "x" * 1800 for i in range(500)}}
output, elapsed = run(large)
thread.join(5)
assert not thread.is_alive() and not errors, errors
assert output == "{}" and 1.5 < elapsed < 3, (output, elapsed)
print(f"PASS unresponsive pipe: relay abandons blocked write in {elapsed:.2f}s")

# Codex can never receive Allow/Deny, even when manually given PermissionRequest.
output, elapsed = run({**payload, "hook_event_name": "PermissionRequest"})
assert output == "" and elapsed < 1
print("PASS unsupported permission event returns no verdict immediately")

# The existing Claude approval relay still returns its documented output.
def allow(handle):
    read(handle)
    answer = b"allow\n"
    n = w.DWORD()
    assert kernel.WriteFile(handle, answer, len(answer), ctypes.byref(n), None)
    time.sleep(.05)
thread, errors = server(allow)
output, _ = run({**payload, "hook_event_name": "PermissionRequest"}, agent="")
thread.join(5)
assert not thread.is_alive() and not errors, errors
assert json.loads(output)["hookSpecificOutput"]["decision"]["behavior"] == "allow"
print("PASS unchanged Claude request/allow relay contract")
