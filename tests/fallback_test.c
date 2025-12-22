/* Test program for local-first fallback behavior
 * Uses relative paths so fallback to remote file server works correctly.
 * Must be run from a directory containing data/local_only.txt
 * File server must serve a directory containing data/remote_only.txt
 */
#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <sys/stat.h>

int main(void) {
    /* Use relative paths - these work for both local and remote */
    const char* local_only_path = "data/local_only.txt";
    const char* remote_only_path = "data/remote_only.txt";
    const char* neither_path = "data/neither.txt";
    char buf[256];

    printf("=== Local-First Fallback Test ===\n\n");

    /* Test 1: File exists locally - should use local */
    printf("Test 1: File exists locally (%s)\n", local_only_path);
    int fd = open(local_only_path, O_RDONLY);
    if (fd < 0) {
        printf("  FAIL: open() failed: %s\n", strerror(errno));
        return 1;
    }
    memset(buf, 0, sizeof(buf));
    read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (strstr(buf, "LOCAL") != NULL) {
        printf("  PASS: Got local content: '%s'\n", buf);
    } else {
        printf("  FAIL: Expected LOCAL content, got: '%s'\n", buf);
        return 1;
    }

    /* Test 2: File doesn't exist locally, exists remotely - should fallback */
    printf("\nTest 2: File exists only remotely (%s)\n", remote_only_path);
    fd = open(remote_only_path, O_RDONLY);
    if (fd < 0) {
        printf("  FAIL: open() failed (fallback didn't work): %s\n", strerror(errno));
        return 1;
    }
    memset(buf, 0, sizeof(buf));
    read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (strstr(buf, "REMOTE") != NULL) {
        printf("  PASS: Got remote content via fallback: '%s'\n", buf);
    } else {
        printf("  FAIL: Expected REMOTE content, got: '%s'\n", buf);
        return 1;
    }

    /* Test 3: File doesn't exist anywhere - should return ENOENT */
    printf("\nTest 3: File doesn't exist anywhere (%s)\n", neither_path);
    fd = open(neither_path, O_RDONLY);
    if (fd >= 0) {
        close(fd);
        printf("  FAIL: open() succeeded but file shouldn't exist\n");
        return 1;
    }
    if (errno == ENOENT) {
        printf("  PASS: Got ENOENT as expected\n");
    } else {
        printf("  FAIL: Expected ENOENT, got: %s\n", strerror(errno));
        return 1;
    }

    /* Test 4: stat() fallback */
    printf("\nTest 4: stat() fallback for remote-only file\n");
    struct stat st;
    if (stat(remote_only_path, &st) == 0) {
        printf("  PASS: stat() succeeded via fallback, size=%ld\n", st.st_size);
    } else {
        printf("  FAIL: stat() failed: %s\n", strerror(errno));
        return 1;
    }

    /* Test 5: access() fallback */
    printf("\nTest 5: access() fallback for remote-only file\n");
    if (access(remote_only_path, R_OK) == 0) {
        printf("  PASS: access() succeeded via fallback\n");
    } else {
        printf("  FAIL: access() failed: %s\n", strerror(errno));
        return 1;
    }

    printf("\n✅ ALL FALLBACK TESTS PASSED!\n");
    return 0;
}
