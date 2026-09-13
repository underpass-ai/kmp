/* Linux/glibc-only informational allocation control for native MCP processes.
 * Build: cc -shared -fPIC -O2 -Wall -Wextra -Werror -o counter.so this-file.c
 * Run: LD_PRELOAD=/absolute/counter.so PROGRAM
 * One JSON line is written to stderr at normal process exit. Counts include
 * startup, warmup and every thread. Requested bytes are cumulative allocation
 * requests, NOT retained bytes, physical memory, or successful allocations.
 * glibc's internal entry points avoid allocating during symbol resolution.
 * This is profiling instrumentation; never preload it into production hosts.
 */
#define _GNU_SOURCE
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

extern void *__libc_malloc(size_t);
extern void *__libc_calloc(size_t, size_t);
extern void *__libc_realloc(void *, size_t);
extern void __libc_free(void *);
extern void *__libc_memalign(size_t, size_t);

static _Atomic uint64_t requests;
static _Atomic uint64_t requested_bytes;
static _Atomic uint64_t frees;

static void count(size_t size) {
    atomic_fetch_add_explicit(&requests, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(&requested_bytes, size, memory_order_relaxed);
}

void *malloc(size_t size) {
    count(size);
    return __libc_malloc(size);
}
void *calloc(size_t n, size_t size) {
    count(n && size > SIZE_MAX / n ? SIZE_MAX : n * size);
    return __libc_calloc(n, size);
}
void *realloc(void *ptr, size_t size) {
    count(size);
    return __libc_realloc(ptr, size);
}
void free(void *ptr) {
    if (ptr) atomic_fetch_add_explicit(&frees, 1, memory_order_relaxed);
    __libc_free(ptr);
}
void *aligned_alloc(size_t alignment, size_t size) {
    count(size);
    return __libc_memalign(alignment, size);
}
void *memalign(size_t alignment, size_t size) {
    count(size);
    return __libc_memalign(alignment, size);
}
int posix_memalign(void **ptr, size_t alignment, size_t size) {
    if (alignment < sizeof(void *) || (alignment & (alignment - 1))) return 22;
    count(size);
    void *allocated = __libc_memalign(alignment, size);
    if (!allocated) return 12;
    *ptr = allocated;
    return 0;
}

__attribute__((destructor)) static void report(void) {
    char line[256];
    int size = snprintf(line, sizeof line,
        "{\"native_allocation_control\":true,\"requests\":%llu,\"requested_bytes\":%llu,\"frees\":%llu}\n",
        (unsigned long long)atomic_load_explicit(&requests, memory_order_relaxed),
        (unsigned long long)atomic_load_explicit(&requested_bytes, memory_order_relaxed),
        (unsigned long long)atomic_load_explicit(&frees, memory_order_relaxed));
    if (size > 0 && (size_t)size < sizeof line) {
        ssize_t written = write(STDERR_FILENO, line, (size_t)size);
        (void)written;
    }
}
