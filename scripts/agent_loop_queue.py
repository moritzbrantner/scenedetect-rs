#!/usr/bin/env python3
from pathlib import Path
import runpy
import sys

SCRIPT = Path.home() / ".codex" / "skills" / "agent-loop" / "scripts" / "agent_loop_queue.py"

if not SCRIPT.exists():
    raise SystemExit(f"Missing agent-loop helper script: {SCRIPT}")

sys.argv[0] = str(SCRIPT)
sys.path.insert(0, str(SCRIPT.parent))
runpy.run_path(str(SCRIPT), run_name="__main__")
