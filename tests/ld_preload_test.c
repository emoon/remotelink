/* Test program for LD_PRELOAD file interception */
#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <sys/stat.h>

int main() {
    printf("=== LD_PRELOAD File Interception Test ===\n\n");

    /* Test 1: stat() */
    printf("Test 1: stat(/host/test.txt)\n");
    struct stat st;
    if (stat("/host/test.txt", &st) == 0) {
        printf("  ✓ stat() succeeded, size=%ld\n", st.st_size);
    } else {
        printf("  ✗ stat() failed\n");
        return 1;
    }

    /* Test 2: open() */
    printf("\nTest 2: open(/host/test.txt)\n");
    int fd = open("/host/test.txt", O_RDONLY);
    if (fd < 0) {
        printf("  ✗ open() failed\n");
        return 1;
    }
    printf("  ✓ open() succeeded, fd=%d\n", fd);

    /* Test 3: read() */
    printf("\nTest 3: read()\n");
    char buf[256];
    memset(buf, 0, sizeof(buf));
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    if (n < 0) {
        printf("  ✗ read() failed\n");
        return 1;
    }
    printf("  ✓ read() succeeded, %ld bytes\n", n);
    printf("  Content: '%s'\n", buf);

    /* Test 4: lseek() */
    printf("\nTest 4: lseek()\n");
    off_t pos = lseek(fd, 0, SEEK_SET);
    if (pos < 0) {
        printf("  ✗ lseek() failed\n");
        return 1;
    }
    printf("  ✓ lseek() succeeded, pos=%ld\n", pos);

    /* Test 5: fstat() */
    printf("\nTest 5: fstat()\n");
    if (fstat(fd, &st) == 0) {
        printf("  ✓ fstat() succeeded, size=%ld\n", st.st_size);
    } else {
        printf("  ✗ fstat() failed\n");
        return 1;
    }

    /* Test 6: close() */
    printf("\nTest 6: close()\n");
    if (close(fd) == 0) {
        printf("  ✓ close() succeeded\n");
    } else {
        printf("  ✗ close() failed\n");
        return 1;
    }

    /* Test 7: non-/host/ paths should work normally */
    printf("\nTest 7: open(/etc/passwd) - should use real syscall\n");
    fd = open("/etc/passwd", O_RDONLY);
    if (fd >= 0) {
        printf("  ✓ open() succeeded for non-/host/ path\n");
        close(fd);
    } else {
        printf("  ✗ open() failed for regular path\n");
        return 1;
    }

    printf("\n✅ ALL LD_PRELOAD TESTS PASSED!\n");
    return 0;
}
