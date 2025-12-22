/* Simple shared library for testing remote .so loading */
#include <stdio.h>

int shared_lib_add(int a, int b) {
    return a + b;
}

const char* shared_lib_get_message(void) {
    return "Hello from remote shared library!";
}
