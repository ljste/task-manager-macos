#include "SystemBridge.h"
#include <libproc.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <time.h>
#include <unistd.h>

static double monotonic_seconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now.tv_sec + now.tv_nsec / 1e9;
}

int main(int argc, char **argv) {
    if (argc != 2) return 1;
    int pid = atoi(argv[1]);
    struct rusage_info_v2 last, current;
    if (proc_pid_rusage(pid, RUSAGE_INFO_V2, (rusage_info_t *)&last)) return 2;
    double previous = monotonic_seconds();
    puts("[");
    for (int i = 0; i < 20; ++i) {
        sleep(1);
        if (proc_pid_rusage(pid, RUSAGE_INFO_V2, (rusage_info_t *)&current)) return 3;
        double now = monotonic_seconds();
        uint64_t ticks = (current.ri_user_time - last.ri_user_time) +
                         (current.ri_system_time - last.ri_system_time);
        double cpu = tm_ticks_to_ns(ticks) / 1e9 / (now - previous) * 100.;
        printf("%s{\"cpu_one_core_percent\":%.5f,\"rss_mib\":%.3f,\"footprint_mib\":%.3f}",
               i ? ",\n" : "", cpu, current.ri_resident_size / 1048576.,
               current.ri_phys_footprint / 1048576.);
        last = current;
        previous = now;
    }
    puts("\n]");
    return 0;
}
