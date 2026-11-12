# `tyyoj-judger`

Persistent judging worker daemon and low-level Linux sandbox engine for **Toi Yêu Yuri Online Judge (`tyyoj`)**.

## Architecture

`tyyoj-judger` runs as a persistent daemon container inside Docker, managed directly by the `tyyoj-backend` Docker Engine provisioner.

- **Rust Worker Daemon (`src/daemon`):**
  Continuously polls submission jobs from Redis, reports live worker telemetry heartbeats, fetches problem test case data, coordinates compilation, and delegates execution to the low-level sandbox.
- **C++20 Linux Sandbox (`src/sandbox`):**
  Uses Linux namespaces (`CLONE_NEWPID`, `CLONE_NEWNET`, `CLONE_NEWNS`, `CLONE_NEWIPC`), cgroups v2 memory/CPU controllers, `rlimit`, and `seccomp-bpf` syscall filters to run contestant binaries securely.
- **Languages Supported:**
  - C++ (`g++ -std=c++20 -O2`)
  - Free Pascal (`fpc -O2`)
  - Python (`python3` / `pypy3`)
- **Judging Modes:**
  - Standard Diff/Token comparison.
  - Custom Checker (`testlib.h` arguments: `inf`, `ouf`, `ans`).
  - Interactive Judge: Bidirectional POSIX pipes between contestant and problem interactor with wall-clock idleness limits.

## License

MIT © 2026 Toi Yêu Yuri Online Judge (toiyeuyuriiiii & nmkdeveloper)
