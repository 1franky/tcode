#include <stdio.h>
#define MAX 10

typedef struct { int x; float y; } punto_t;

static int sumar(const int *a, size_t n) {
    int total = 0; /* comentario */
    for (size_t i = 0; i < n; ++i) total += a[i];
    return total;
}

int main(void) {
    punto_t p = { .x = 1, .y = 2.5f };
    printf("%d %c\n", sumar(&p.x, 1), 'z');
    return p.x > MAX ? 1 : 0;
}
