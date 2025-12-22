/* Test program for remote shared library loading via dlopen */
#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>
#include <string.h>

typedef int (*add_func)(int, int);
typedef const char* (*msg_func)(void);

int main() {
    printf("=== Remote Shared Library Loading Test ===\n\n");

    /* Test 1: dlopen() from /host/ path */
    printf("Test 1: dlopen(/host/libs/libshared_test.so)\n");
    void* handle = dlopen("/host/libs/libshared_test.so", RTLD_NOW);
    if (!handle) {
        printf("  ✗ dlopen() failed: %s\n", dlerror());
        return 1;
    }
    printf("  ✓ dlopen() succeeded, handle=%p\n", handle);

    /* Test 2: dlsym() for add function */
    printf("\nTest 2: dlsym(shared_lib_add)\n");
    dlerror(); /* Clear errors */
    add_func add = (add_func)dlsym(handle, "shared_lib_add");
    char* error = dlerror();
    if (error) {
        printf("  ✗ dlsym() failed: %s\n", error);
        dlclose(handle);
        return 1;
    }
    printf("  ✓ dlsym() succeeded, func=%p\n", (void*)add);

    /* Test 3: Call the add function */
    printf("\nTest 3: Call shared_lib_add(3, 4)\n");
    int result = add(3, 4);
    if (result != 7) {
        printf("  ✗ Expected 7, got %d\n", result);
        dlclose(handle);
        return 1;
    }
    printf("  ✓ Result: %d (correct!)\n", result);

    /* Test 4: dlsym() for message function */
    printf("\nTest 4: dlsym(shared_lib_get_message)\n");
    dlerror();
    msg_func get_msg = (msg_func)dlsym(handle, "shared_lib_get_message");
    error = dlerror();
    if (error) {
        printf("  ✗ dlsym() failed: %s\n", error);
        dlclose(handle);
        return 1;
    }
    printf("  ✓ dlsym() succeeded\n");

    /* Test 5: Call the message function */
    printf("\nTest 5: Call shared_lib_get_message()\n");
    const char* msg = get_msg();
    if (!msg || strcmp(msg, "Hello from remote shared library!") != 0) {
        printf("  ✗ Unexpected message: %s\n", msg ? msg : "(null)");
        dlclose(handle);
        return 1;
    }
    printf("  ✓ Message: '%s'\n", msg);

    /* Test 6: dlclose() */
    printf("\nTest 6: dlclose()\n");
    if (dlclose(handle) != 0) {
        printf("  ✗ dlclose() failed: %s\n", dlerror());
        return 1;
    }
    printf("  ✓ dlclose() succeeded\n");

    printf("\n✅ ALL REMOTE SHARED LIBRARY TESTS PASSED!\n");
    return 0;
}
