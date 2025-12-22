/* Test program for implicit shared library linking via prefetch */
/* Links against libshared_test.so at compile time */
#include <stdio.h>

/* Declare functions from the shared library */
extern int shared_lib_add(int a, int b);
extern const char* shared_lib_get_message(void);

int main() {
    printf("=== Implicit Shared Library Test ===\n\n");

    printf("Test 1: Call shared_lib_add(5, 3)\n");
    int result = shared_lib_add(5, 3);
    if (result != 8) {
        printf("  FAIL: Expected 8, got %d\n", result);
        return 1;
    }
    printf("  PASS: Result: %d\n", result);

    printf("\nTest 2: Call shared_lib_get_message()\n");
    const char* msg = shared_lib_get_message();
    if (!msg) {
        printf("  FAIL: Got null message\n");
        return 1;
    }
    printf("  PASS: Message: '%s'\n", msg);

    printf("\nALL TESTS PASSED!\n");
    return 0;
}
