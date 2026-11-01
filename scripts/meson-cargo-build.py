import os
from pathlib import Path
import shutil
import subprocess
import sys

source, profile, kerberos, gui, agent, askpass = sys.argv[1:]
arguments = ["cargo", "build", "--manifest-path", str(Path(source) / "Cargo.toml"),
             "--locked", "-p", "bookie-app", "-p", "bookie-agentd", "-p", "bookie-ssh", "--bin", "bookie-app",
             "--bin", "bookie-agentd", "--bin", "bookie-askpass"]
if profile == "default":
    arguments.append("--release")
if kerberos == "false":
    arguments.append("--no-default-features")
subprocess.run(arguments, cwd=source, check=True)
built = Path(os.environ["CARGO_TARGET_DIR"]) / ("release" if profile == "default" else "debug")
shutil.copy2(built / "bookie-app", gui)
shutil.copy2(built / "bookie-agentd", agent)
shutil.copy2(built / "bookie-askpass", askpass)
