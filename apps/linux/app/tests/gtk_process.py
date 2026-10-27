from pathlib import Path
import subprocess


class OwnedApplication:
    def __init__(self, process, executable, environment, proc_root=Path("/proc")):
        self.process = process
        self.executable = Path(executable).resolve()
        self._environment = environment.copy()
        self.proc = Path(proc_root) / str(process.pid)
        try:
            self._executable_identity = self._file_identity(self.executable)
            self.started = self._start_time()
        except (OSError, ValueError, IndexError):
            self._executable_identity = None
            self.started = None

    @staticmethod
    def _file_identity(path):
        metadata = path.stat()
        return metadata.st_dev, metadata.st_ino

    def _start_time(self):
        fields = (self.proc / "stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[19])

    def _verify(self):
        if self.process.poll() is not None:
            raise AssertionError("the owned application has exited")
        try:
            if self.started is None or self._start_time() != self.started:
                raise AssertionError("the owned process identity changed")
            actual = self._file_identity(self.proc / "exe")
            expected = self._file_identity(self.executable)
            if actual != self._executable_identity or expected != self._executable_identity:
                raise AssertionError("the owned executable changed")
        except (OSError, ValueError, IndexError):
            raise AssertionError("the owned application identity is unavailable") from None
        if self.process.poll() is not None:
            raise AssertionError("the owned application has exited")

    def memory_kb(self):
        self._verify()
        try:
            text = (self.proc / "status").read_text()
            fields = dict(line.split(":", 1) for line in text.splitlines() if ":" in line)
            memory = tuple(int(fields[name].split()[0]) for name in ("VmRSS", "VmHWM"))
        except (OSError, KeyError, ValueError, IndexError):
            raise AssertionError("the owned application memory is unavailable") from None
        self._verify()
        return memory

    def identity(self):
        self._verify()
        return {"application_pid": self.process.pid, "application_start_ticks": self.started}

    def assert_owns_tcp_listener(self, port):
        self._verify()
        try:
            sockets = {entry.readlink().name for entry in (self.proc / "fd").iterdir()}
            endpoint = f"0100007F:{port:04X}"
            listeners = [line.split() for line in (self.proc / "net/tcp").read_text().splitlines()[1:]]
            owned = any(
                len(fields) > 9 and fields[1] == endpoint and fields[3] == "0A"
                and f"socket:[{fields[9]}]" in sockets
                for fields in listeners
            )
        except OSError:
            raise AssertionError("the owned application's TCP listener cannot be verified") from None
        self._verify()
        if not owned:
            raise AssertionError("the MCP test port is not listening in the owned application")

    def launch_again(self):
        self._verify()
        result = subprocess.run(
            [str(self.executable)], env=self._environment.copy(), capture_output=True, text=True, timeout=30
        )
        self._verify()
        return result
