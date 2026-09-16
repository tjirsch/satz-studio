#!/usr/bin/env python3
"""A stand-in for the Claude Code CLI, for the offline `claude_code_*` tests.

It answers `--version` and `auth status`, and in `-p` mode it speaks the stdio control
protocol: it checks the arguments the app built, answers the initialize request, and
replays one recorded stream per user message.

One test's settings are one JSON file, named by `FAKE_CLAUDE_CONFIG`; each test writes
its own and a small wrapper script sets the variable, so nothing is shared between
tests running at the same time. Its keys, all optional:

  version      what `--version` prints              (default "2.1.270 (Claude Code)")
  auth         what `auth status --json` prints     (default: signed out)
  args_out     a file the received argv is written to, as JSON
  stdin_out    a file every line read from stdin is appended to
  scripts      recorded streams, one per user turn
  mcp_status   the status `system/init` reports for every MCP server ("connected")
  model        the model `system/init` reports       (default: --model, else opus)

A recorded stream is one JSON object per line, each of:

  {"emit": {...}}                                   print that object
  {"stderr": "<text>"}                              write that line to stderr
  {"await_control": "<id>", "allow": [...], "deny": [...]}
                                                    block until the client answers the
                                                    control request <id>, then print
                                                    the objects of the branch it took
  {"await_interrupt": true, "then": [...]}          block until the client interrupts
                                                    the turn, then print the objects

Every object printed carries the session id, so the app reads one session throughout.
The account address never appears here: a fixture uses first.admin@example.com.
"""

import json
import os
import sys
import threading

SESSION_ID = "11111111-1111-1111-1111-111111111111"

# what the app must pass, or the session is not the one this app builds
REQUIRED = [
    ("-p", None),
    ("--input-format", "stream-json"),
    ("--output-format", "stream-json"),
    ("--setting-sources", ""),
    ("--strict-mcp-config", None),
    ("--tools", ""),
    ("--permission-mode", "default"),
    ("--permission-prompt-tool", "stdio"),
]


def fail(message):
    print(f"fake-claude: {message}", file=sys.stderr)
    sys.exit(3)


def settings():
    path = os.environ.get("FAKE_CLAUDE_CONFIG")
    if not path:
        return {}
    with open(path, encoding="utf-8") as f:
        return json.load(f)


CONFIG = settings()


def check(argv):
    for flag, value in REQUIRED:
        if flag not in argv:
            fail(f"the command line is missing {flag}: {argv}")
        if value is not None and argv[argv.index(flag) + 1] != value:
            fail(f"{flag} is {argv[argv.index(flag) + 1]!r}, not {value!r}")
    if "--mcp-config" not in argv:
        fail("the command line names no MCP server")
    if "--append-system-prompt" not in argv:
        fail("the command line carries no system prompt")


def auth_status():
    status = CONFIG.get("auth", {"loggedIn": False, "authMethod": None, "apiProvider": None})
    json.dump(status, sys.stdout)
    sys.stdout.write("\n")
    return 0


class Client:
    """The lines the client writes, and what they answer."""

    def __init__(self, log):
        self.log = log
        self.initialized = threading.Event()
        self.interrupted = threading.Event()
        self.answers = {}
        self.turns = []
        self.answered = threading.Condition()

    def read_forever(self):
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            if self.log:
                with open(self.log, "a", encoding="utf-8") as f:
                    f.write(line + "\n")
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                fail(f"the client wrote a line that is not JSON: {line!r}")
            self.handle(message)

    def handle(self, message):
        kind = message.get("type")
        if kind == "control_request":
            request = message.get("request", {})
            if request.get("subtype") == "initialize":
                emit({
                    "type": "control_response",
                    "response": {
                        "subtype": "success",
                        "request_id": message["request_id"],
                        "response": {
                            "commands": [],
                            "account": {"email": "first.admin@example.com"},
                        },
                    },
                })
                self.initialized.set()
            elif request.get("subtype") == "interrupt":
                emit({
                    "type": "control_response",
                    "response": {
                        "subtype": "success",
                        "request_id": message["request_id"],
                        "response": {},
                    },
                })
                self.interrupted.set()
        elif kind == "control_response":
            response = message.get("response", {})
            with self.answered:
                self.answers[response.get("request_id")] = response.get("response", {})
                self.answered.notify_all()
        elif kind == "user":
            with self.answered:
                self.turns.append(message)
                self.answered.notify_all()

    def next_turn(self, index, timeout=60):
        with self.answered:
            if not self.answered.wait_for(lambda: len(self.turns) > index, timeout=timeout):
                fail(f"the client never sent user message {index}")
            return self.turns[index]

    def wait_for(self, request_id, timeout=60):
        with self.answered:
            if not self.answered.wait_for(lambda: request_id in self.answers, timeout=timeout):
                fail(f"the client never answered control request {request_id}")
            return self.answers[request_id]


def emit(obj):
    obj.setdefault("session_id", SESSION_ID)
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def init_line(argv):
    model = CONFIG.get("model")
    if not model and "--model" in argv:
        model = argv[argv.index("--model") + 1]
    config = json.loads(argv[argv.index("--mcp-config") + 1])
    status = CONFIG.get("mcp_status", "connected")
    return {
        "type": "system",
        "subtype": "init",
        "cwd": os.getcwd(),
        "model": model or "claude-opus-5",
        "permissionMode": "default",
        "apiKeySource": "none",
        "mcp_servers": [{"name": name, "status": status} for name in config["mcpServers"]],
        "tools": [],
    }


def replay(path, client):
    with open(path, encoding="utf-8") as f:
        steps = [json.loads(line) for line in f if line.strip()]
    for step in steps:
        if "emit" in step:
            emit(step["emit"])
        elif "stderr" in step:
            sys.stderr.write(step["stderr"] + "\n")
            sys.stderr.flush()
        elif "await_interrupt" in step:
            if not client.interrupted.wait(timeout=60):
                fail("the client never sent the interrupt request")
            for obj in step.get("then", []):
                emit(obj)
        elif "await_control" in step:
            answer = client.wait_for(step["await_control"])
            branch = "allow" if answer.get("behavior") == "allow" else "deny"
            for obj in step.get(branch, []):
                emit(obj)
        else:
            fail(f"a step this fake does not know: {step}")


def session(argv):
    check(argv)
    if CONFIG.get("args_out"):
        with open(CONFIG["args_out"], "w", encoding="utf-8") as f:
            json.dump(argv, f)
    client = Client(CONFIG.get("stdin_out"))
    reader = threading.Thread(target=client.read_forever, daemon=True)
    reader.start()
    if not client.initialized.wait(timeout=60):
        fail("the client never sent the initialize request")
    for index, script in enumerate(CONFIG.get("scripts", [])):
        client.next_turn(index)
        if index == 0:
            emit(init_line(argv))
        replay(script, client)
    reader.join(timeout=5)
    return 0


def main():
    argv = sys.argv[1:]
    if "--version" in argv:
        print(CONFIG.get("version", "2.1.270 (Claude Code)"))
        return 0
    if argv[:2] == ["auth", "status"]:
        return auth_status()
    if argv[:2] in (["auth", "login"], ["auth", "logout"]):
        return 0
    if "-p" in argv:
        return session(argv)
    fail(f"a command this fake does not serve: {argv}")


if __name__ == "__main__":
    sys.exit(main())
