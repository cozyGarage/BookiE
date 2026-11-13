#define _GNU_SOURCE
#include <fcntl.h>
#include <malloc.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <unistd.h>

static void *trim_worker(void *unused) {
    (void)unused;
    sigset_t signals;
    sigemptyset(&signals);
    sigaddset(&signals, SIGUSR2);
    for (;;) {
        int signal_number = 0;
        if (sigwait(&signals, &signal_number) != 0) continue;
        int released = malloc_trim(0);
        const char *path = getenv("BOOKIE_PROFILE_MALLOC_TRIM_RESULT");
        if (path == NULL) continue;
        int descriptor = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC | O_NOFOLLOW, 0600);
        if (descriptor < 0) continue;
        fchmod(descriptor, 0600);
        char record[32];
        int length = snprintf(record, sizeof(record), "malloc_trim=%d\n", released);
        if (length > 0 && (size_t)length < sizeof(record)) (void)write(descriptor, record, (size_t)length);
        close(descriptor);
    }
    return NULL;
}

__attribute__((constructor)) static void start_trim_worker(void) {
    sigset_t signals;
    sigemptyset(&signals);
    sigaddset(&signals, SIGUSR2);
    if (pthread_sigmask(SIG_BLOCK, &signals, NULL) != 0) return;
    pthread_t worker;
    if (pthread_create(&worker, NULL, trim_worker, NULL) == 0) pthread_detach(worker);
}
