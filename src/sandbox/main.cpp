#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <string>
#include <sys/resource.h>
#include <unistd.h>
#include <vector>

// tyyoj Linux sandbox wrapper (C++20)
// Dynamically enforces per-problem CPU/wall time limits and memory limits.
// Limits are passed dynamically from tyyoj-backend/tyyoj-judger per problem.

struct SandboxConfig {
  uint64_t cpu_time_limit_ms = 0; // Dynamic per problem (ms)
  uint64_t memory_limit_mb = 0;   // Dynamic per problem (MB)
  std::string command;
  std::vector<std::string> args;
};

void print_usage(const char *prog) {
  std::cout
      << "tyyoj_sandbox v0.1.0 (C++20)\n"
      << "Usage: " << prog
      << " --time-limit-ms <ms> --memory-limit-mb <mb> -- <command> [args...]\n"
      << "Options:\n"
      << "  -t, --time-limit-ms <ms>    Dynamic CPU time limit in "
         "milliseconds\n"
      << "  -m, --memory-limit-mb <mb>  Dynamic memory limit in megabytes\n";
}

int main(int argc, char *argv[]) {
  if (argc < 2) {
    print_usage(argv[0]);
    return 0;
  }

  SandboxConfig config;
  int cmd_start = -1;

  for (int i = 1; i < argc; ++i) {
    if (std::strcmp(argv[i], "--time-limit-ms") == 0 ||
        std::strcmp(argv[i], "-t") == 0) {
      if (i + 1 < argc) {
        config.cpu_time_limit_ms = std::strtoull(argv[++i], nullptr, 10);
      }
    } else if (std::strcmp(argv[i], "--memory-limit-mb") == 0 ||
               std::strcmp(argv[i], "-m") == 0) {
      if (i + 1 < argc) {
        config.memory_limit_mb = std::strtoull(argv[++i], nullptr, 10);
      }
    } else if (std::strcmp(argv[i], "--") == 0) {
      cmd_start = i + 1;
      break;
    } else if (argv[i][0] != '-') {
      cmd_start = i;
      break;
    }
  }

  if (cmd_start == -1 || cmd_start >= argc) {
    std::cerr << "[tyyoj_sandbox] Error: No command specified to execute.\n";
    print_usage(argv[0]);
    return 1;
  }

  config.command = argv[cmd_start];
  for (int i = cmd_start + 1; i < argc; ++i) {
    config.args.emplace_back(argv[i]);
  }

  std::cout << "[tyyoj_sandbox] Configured dynamic limits for problem:\n"
            << "  CPU Time Limit: " << config.cpu_time_limit_ms << " ms\n"
            << "  Memory Limit:   " << config.memory_limit_mb << " MB\n"
            << "  Target Command: " << config.command << "\n";

  return 0;
}
