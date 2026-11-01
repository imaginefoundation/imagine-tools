#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv)
{
    const char *name = argc > 1 ? argv[1] : "ImagineOS";
    char *message = malloc(64);
    if (!message) {
        fputs("hello_c: out of memory\n", stderr);
        return 1;
    }
    snprintf(message, 64, "Hello from C, %s!", name);
    puts(message);
    free(message);
    return 0;
}
