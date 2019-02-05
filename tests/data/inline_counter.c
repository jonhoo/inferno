#define COUNT_N 10000000

inline void __attribute__((always_inline)) count_to(unsigned int n) {
    for(int i = 0; i < n; i++);
}

inline void __attribute__((always_inline)) double_inline() {
    count_to(COUNT_N);
}

static void __attribute__ ((noinline)) not_inlined() {
    count_to(COUNT_N);
}

int main() {
    count_to(COUNT_N);
    double_inline();
    not_inlined();
}
