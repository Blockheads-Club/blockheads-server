/* libm_probe: dumps bit-pattern vectors of the 14 libm functions imported by the original server.
 * Build (inside bh-libm-probe:14.04, glibc 2.19): gcc -std=gnu99 -O0 -fno-builtin -o libm_probe libm_probe.c -lm
 *
 *   libm_probe sweep [seed N] > libm_sweep.bin   deterministic argument sets (xorshift64, default seed 88172645463325252)
 *   libm_probe eval  < args.txt > out.bin  evaluate "fn_id hexarg [hexarg]" lines
 *
 * Record format (little-endian): u8 fn_id; u8 nargs; args (u32 for f32 functions, u64 for f64 ones); result bits.
 * fn_id: 0 sin 1 cos 2 pow 3 atan2 4 floor 5 fmod 6 sqrt 7 sinf 8 cosf 9 tanf 10 powf 11 atan2f 12 fmodf 13 sqrtf
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>

static uint64_t rng_state = 88172645463325252ULL;
static uint64_t xs(void) {
    uint64_t x = rng_state;
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    return rng_state = x;
}
static double urand(void) { return (double)(xs() >> 11) * (1.0 / 9007199254740992.0); }

static const int NARGS[14] = {1,1,2,2,1,2,1,1,1,1,2,2,2,1};
static const int IS64[14]  = {1,1,1,1,1,1,1,0,0,0,0,0,0,0};

static uint32_t f2u(float f) { uint32_t u; memcpy(&u, &f, 4); return u; }
static float u2f(uint32_t u) { float f; memcpy(&f, &u, 4); return f; }
static uint64_t d2u(double f) { uint64_t u; memcpy(&u, &f, 8); return u; }
static double u2d(uint64_t u) { double f; memcpy(&f, &u, 8); return f; }

static void emit(int fn, uint64_t a, uint64_t b) {
    uint64_t r = 0;
    if (IS64[fn]) {
        double x = u2d(a), y = u2d(b), z = 0;
        switch (fn) {
        case 0: z = sin(x); break; case 1: z = cos(x); break;
        case 2: z = pow(x, y); break; case 3: z = atan2(x, y); break;
        case 4: z = floor(x); break; case 5: z = fmod(x, y); break;
        case 6: z = sqrt(x); break;
        }
        r = d2u(z);
    } else {
        float x = u2f((uint32_t)a), y = u2f((uint32_t)b), z = 0;
        switch (fn) {
        case 7: z = sinf(x); break; case 8: z = cosf(x); break; case 9: z = tanf(x); break;
        case 10: z = powf(x, y); break; case 11: z = atan2f(x, y); break;
        case 12: z = fmodf(x, y); break; case 13: z = sqrtf(x); break;
        }
        r = f2u(z);
    }
    uint8_t hdr[2] = { (uint8_t)fn, (uint8_t)NARGS[fn] };
    fwrite(hdr, 1, 2, stdout);
    if (IS64[fn]) {
        fwrite(&a, 8, 1, stdout);
        if (NARGS[fn] == 2) fwrite(&b, 8, 1, stdout);
        fwrite(&r, 8, 1, stdout);
    } else {
        uint32_t a32 = (uint32_t)a, b32 = (uint32_t)b, r32 = (uint32_t)r;
        fwrite(&a32, 4, 1, stdout);
        if (NARGS[fn] == 2) fwrite(&b32, 4, 1, stdout);
        fwrite(&r32, 4, 1, stdout);
    }
}

static uint64_t mk(int fn, double v) { return IS64[fn] ? d2u(v) : (uint64_t)f2u((float)v); }
static double uni(double lo, double hi) { return lo + (hi - lo) * urand(); }

static int SWEEP_N = 14000;
static void sweep(void) {
    const int N = SWEEP_N;
    static const double fmod_div[] = {0.25, 0.5, 1.0, 0.125, 900.0};
    for (int fn = 0; fn < 14; fn++) {
        int two = NARGS[fn] == 2;
        /* (a) uniform in used ranges */
        for (int i = 0; i < N; i++) {
            double a, b = 0;
            switch (fn) {
            case 0: case 1: case 7: case 8: a = uni(-40, 40); break;
            case 9: a = uni(-10, 10); break;
            case 2: case 10: a = uni(0, 4); b = (fn == 10 && (i & 1)) ? ((i & 2) ? 0.5 : 2.0) : uni(-40, 40); break;
            case 3: case 11: a = uni(-5, 5); b = uni(-5, 5); break;
            case 4: a = uni(-1000, 1000); break;
            case 5: case 12: a = uni(-1000, 1000); b = fmod_div[i % 5]; break;
            default: a = uni(0, 1000); break;
            }
            emit(fn, mk(fn, a), two ? mk(fn, b) : 0);
        }
        /* (a2) small-range extras: powf/pow typical shapes */
        if (fn == 10) for (int i = 0; i < 4000; i++) emit(fn, mk(fn, 10.0 * uni(0, 0.1)), mk(fn, 2.0));
        if (fn == 10) for (int i = 0; i < 2000; i++) emit(fn, mk(fn, 0.8), mk(fn, (uni(0, 1) - 0.5) * 10));
        if (fn == 2) for (int i = 0; i < 4000; i++) emit(fn, mk(fn, 0.999), mk(fn, (double)(int)uni(-5000, 5000)));
        if (fn == 2) for (int i = 0; i < 2000; i++) emit(fn, mk(fn, uni(0.5, 2)), mk(fn, (double)(int)uni(2, 6)));
        if (fn == 0 || fn == 1) for (int i = 0; i < 4000; i++) emit(fn, mk(fn, uni(-8, 8)), 0);
        if (fn == 7 || fn == 8) for (int i = 0; i < 4000; i++) emit(fn, mk(fn, uni(-8, 8)), 0);
        /* (b) random full-width bit patterns */
        for (int i = 0; i < 4000; i++) {
            uint64_t a = xs(), b = xs();
            if (!IS64[fn]) { a &= 0xffffffffu; b &= 0xffffffffu; }
            emit(fn, a, two ? b : 0);
        }
        /* (d) large-argument regions */
        if (fn == 0 || fn == 1 || fn == 7 || fn == 8 || fn == 9) {
            for (int i = 0; i < 3000; i++) {
                double mag = pow(10.0, uni(0, fn <= 1 ? 22 : 38.5));
                if (!IS64[fn] && mag > 3e38) mag = 3e38;
                emit(fn, mk(fn, (i & 1) ? mag : -mag), 0);
            }
            for (int e = 18; e <= 25; e++) for (int i = 0; i < 100; i++) {
                double v = ldexp(1.0 + urand(), e);
                emit(fn, mk(fn, v), 0);
            }
        }
        /* (c) edge table */
        static const double edge[] = {0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 3.0, 10.0, 0.001, 0.999, 1.0000001, 100.0, 1e-10, 1e-30,
            3.141592653589793, 6.283185307179586, 1.5707963267948966, 0.7853981633974483, 2.356194490192345, 4.71238898038469,
            7.0685834705770345, 1048576.0, 8388608.0, 1e9, 3e38, 1e300, 1e-300, 4.9e-324, 2.2250738585072014e-308, 1.1754944e-38, 1e-45};
        int NE = sizeof(edge) / sizeof(edge[0]);
        for (int i = 0; i < NE; i++) for (int s = 0; s < 2; s++) {
            double v = s ? -edge[i] : edge[i];
            for (int j = 0; j < (two ? NE : 1); j++) for (int t = 0; t < (two ? 2 : 1); t++) {
                double w = t ? -edge[j] : edge[j];
                emit(fn, mk(fn, v), two ? mk(fn, w) : 0);
            }
        }
        /* inf / nan / extremes */
        uint64_t sp64[] = {0x7ff0000000000000ULL, 0xfff0000000000000ULL, 0x7ff8000000000000ULL, 0xfff8000000000000ULL,
                           0x7ff0000000000001ULL, 0x7fefffffffffffffULL, 0x8000000000000000ULL, 0x0000000000000001ULL, 0x000fffffffffffffULL};
        uint32_t sp32[] = {0x7f800000u, 0xff800000u, 0x7fc00000u, 0xffc00000u, 0x7f800001u, 0x7f7fffffu, 0x80000000u, 1u, 0x007fffffu, 0x00800000u};
        if (IS64[fn]) {
            int n = sizeof(sp64) / 8;
            for (int i = 0; i < n; i++) for (int j = 0; j < (two ? n : 1); j++) emit(fn, sp64[i], two ? sp64[j] : 0);
            for (int i = 0; i < n; i++) for (int j = 0; j < (two ? 9 : 1); j++) emit(fn, sp64[i], two ? d2u(edge[j]) : 0);
            if (two) for (int i = 0; i < n; i++) for (int j = 0; j < 9; j++) emit(fn, d2u(edge[j]), sp64[i]);
        } else {
            int n = sizeof(sp32) / 4;
            for (int i = 0; i < n; i++) for (int j = 0; j < (two ? n : 1); j++) emit(fn, sp32[i], two ? sp32[j] : 0);
            for (int i = 0; i < n; i++) for (int j = 0; j < (two ? 9 : 1); j++) emit(fn, sp32[i], two ? f2u((float)edge[j]) : 0);
            if (two) for (int i = 0; i < n; i++) for (int j = 0; j < 9; j++) emit(fn, f2u((float)edge[j]), sp32[i]);
        }
        /* pi/2 multiples as floats and doubles, neighbours */
        if (fn == 0 || fn == 1 || fn == 7 || fn == 8 || fn == 9) {
            for (int k = 0; k < 200; k++) {
                double v = k * 1.5707963267948966;
                for (int d = -2; d <= 2; d++) {
                    if (IS64[fn]) { emit(fn, d2u(v) + d, 0); emit(fn, d2u(-v) + d, 0); }
                    else { emit(fn, (uint64_t)(f2u((float)v) + d), 0); emit(fn, (uint64_t)(f2u((float)-v) + d), 0); }
                }
            }
        }
    }
}

static void eval(void) {
    char line[256];
    while (fgets(line, sizeof line, stdin)) {
        unsigned fn; unsigned long long a = 0, b = 0;
        int n = sscanf(line, "%u %llx %llx", &fn, &a, &b);
        if (n < 2 || fn > 13) continue;
        emit((int)fn, a, b);
    }
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: libm_probe sweep|eval\n"); return 2; }
    if (!strcmp(argv[1], "sweep")) {
        /* optional: sweep <seed> <N> for larger independent streams */
        if (argc > 2) rng_state = strtoull(argv[2], 0, 10) * 2654435761ULL + 88172645463325252ULL;
        if (argc > 3) SWEEP_N = atoi(argv[3]);
        sweep();
    }
    else if (!strcmp(argv[1], "eval")) eval();
    else return 2;
    return 0;
}
