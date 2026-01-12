/*
 * Event Generator for Benchmark Testing
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Generates file system events at a specified rate for benchmarking
 * the Argus and Janus daemons.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>
#include <sys/stat.h>
#include <dirent.h>
#include <pthread.h>
#include <signal.h>
#include <errno.h>
#include <getopt.h>

#define MAX_PATH 4096
#define MAX_FILES 10000

static volatile int running = 1;
static long total_events = 0;
static pthread_mutex_t counter_mutex = PTHREAD_MUTEX_INITIALIZER;

typedef struct {
    char *directory;
    int rate;
    int duration;
    int thread_id;
} generator_args_t;

void signal_handler(int sig) {
    (void)sig;
    running = 0;
}

/* Get current time in nanoseconds */
long long current_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + ts.tv_nsec;
}

/* Sleep for specified nanoseconds */
void sleep_ns(long long ns) {
    struct timespec ts;
    ts.tv_sec = ns / 1000000000LL;
    ts.tv_nsec = ns % 1000000000LL;
    nanosleep(&ts, NULL);
}

/* Generate file events in a directory */
void *generate_events(void *arg) {
    generator_args_t *args = (generator_args_t *)arg;
    char filepath[MAX_PATH];
    int fd;
    int file_index = 0;
    long local_events = 0;

    /* Calculate interval between events in nanoseconds */
    long long interval_ns = 1000000000LL / args->rate;
    long long start_time = current_time_ns();
    long long end_time = start_time + (long long)args->duration * 1000000000LL;

    while (running && current_time_ns() < end_time) {
        /* Generate different event types */
        int event_type = file_index % 4;

        snprintf(filepath, sizeof(filepath), "%s/bench_t%d_%d.tmp",
                 args->directory, args->thread_id, file_index % 100);

        switch (event_type) {
            case 0: /* Create */
                fd = open(filepath, O_CREAT | O_WRONLY | O_TRUNC, 0644);
                if (fd >= 0) {
                    write(fd, "test", 4);
                    close(fd);
                    local_events++;
                }
                break;

            case 1: /* Modify */
                fd = open(filepath, O_WRONLY | O_APPEND);
                if (fd >= 0) {
                    write(fd, "data", 4);
                    close(fd);
                    local_events++;
                }
                break;

            case 2: /* Read (access) */
                fd = open(filepath, O_RDONLY);
                if (fd >= 0) {
                    char buf[64];
                    read(fd, buf, sizeof(buf));
                    close(fd);
                    local_events++;
                }
                break;

            case 3: /* Delete */
                if (unlink(filepath) == 0) {
                    local_events++;
                }
                break;
        }

        file_index++;

        /* Rate limiting */
        long long next_event = start_time + (local_events * interval_ns);
        long long now = current_time_ns();
        if (next_event > now) {
            sleep_ns(next_event - now);
        }
    }

    /* Update global counter */
    pthread_mutex_lock(&counter_mutex);
    total_events += local_events;
    pthread_mutex_unlock(&counter_mutex);

    return NULL;
}

void print_usage(const char *prog) {
    printf("Usage: %s [OPTIONS]\n", prog);
    printf("\n");
    printf("Options:\n");
    printf("  --dir DIR          Target directory for events\n");
    printf("  --rate RATE        Events per second (default: 1000)\n");
    printf("  --duration SECS    Duration in seconds (default: 60)\n");
    printf("  --threads N        Number of threads (default: 1)\n");
    printf("  -h, --help         Show this help message\n");
    printf("\n");
    printf("Output: Total events generated (printed on last line)\n");
}

int main(int argc, char *argv[]) {
    char *directory = "/tmp/bench";
    int rate = 1000;
    int duration = 60;
    int num_threads = 1;

    static struct option long_options[] = {
        {"dir",      required_argument, 0, 'd'},
        {"rate",     required_argument, 0, 'r'},
        {"duration", required_argument, 0, 't'},
        {"threads",  required_argument, 0, 'n'},
        {"help",     no_argument,       0, 'h'},
        {0, 0, 0, 0}
    };

    int opt;
    int option_index = 0;

    while ((opt = getopt_long(argc, argv, "d:r:t:n:h", long_options, &option_index)) != -1) {
        switch (opt) {
            case 'd':
                directory = optarg;
                break;
            case 'r':
                rate = atoi(optarg);
                break;
            case 't':
                duration = atoi(optarg);
                break;
            case 'n':
                num_threads = atoi(optarg);
                break;
            case 'h':
                print_usage(argv[0]);
                return 0;
            default:
                print_usage(argv[0]);
                return 1;
        }
    }

    /* Validate arguments */
    if (rate <= 0 || duration <= 0 || num_threads <= 0) {
        fprintf(stderr, "Error: Invalid arguments\n");
        return 1;
    }

    /* Create directory if it doesn't exist */
    mkdir(directory, 0755);

    /* Setup signal handler */
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    fprintf(stderr, "Event Generator\n");
    fprintf(stderr, "  Directory: %s\n", directory);
    fprintf(stderr, "  Rate: %d events/sec\n", rate);
    fprintf(stderr, "  Duration: %d seconds\n", duration);
    fprintf(stderr, "  Threads: %d\n", num_threads);
    fprintf(stderr, "\n");

    /* Create worker threads */
    pthread_t *threads = malloc(num_threads * sizeof(pthread_t));
    generator_args_t *args = malloc(num_threads * sizeof(generator_args_t));

    int rate_per_thread = rate / num_threads;

    for (int i = 0; i < num_threads; i++) {
        args[i].directory = directory;
        args[i].rate = rate_per_thread;
        args[i].duration = duration;
        args[i].thread_id = i;

        pthread_create(&threads[i], NULL, generate_events, &args[i]);
    }

    /* Wait for threads to complete */
    for (int i = 0; i < num_threads; i++) {
        pthread_join(threads[i], NULL);
    }

    free(threads);
    free(args);

    /* Clean up temporary files */
    DIR *dir = opendir(directory);
    if (dir) {
        struct dirent *entry;
        char filepath[MAX_PATH];
        while ((entry = readdir(dir)) != NULL) {
            if (strstr(entry->d_name, "bench_t") && strstr(entry->d_name, ".tmp")) {
                snprintf(filepath, sizeof(filepath), "%s/%s", directory, entry->d_name);
                unlink(filepath);
            }
        }
        closedir(dir);
    }

    fprintf(stderr, "Complete. Total events: %ld\n", total_events);

    /* Output total events for script parsing */
    printf("%ld\n", total_events);

    return 0;
}
