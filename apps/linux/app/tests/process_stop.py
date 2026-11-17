import signal
import subprocess


def terminate(process, grace_seconds=5, kill_seconds=5):
    if process.poll() is not None:
        return
    process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=grace_seconds)
        return
    except subprocess.TimeoutExpired:
        process.kill()
    try:
        process.wait(timeout=kill_seconds)
    except subprocess.TimeoutExpired as error:
        raise AssertionError(
            f"application {process.pid} did not exit within {kill_seconds}s of SIGKILL"
        ) from error
