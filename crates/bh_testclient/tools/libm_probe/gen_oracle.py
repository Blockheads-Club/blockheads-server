"""Independent oracle for sin/cos/pow/atan2: mpmath (400 bit) rounded to nearest f64.
Record format identical to libm_probe: u8 fn_id; u8 nargs; args u64 bits; result u64 bits."""
import struct, random, sys
from mpmath import mp, mpf, sin, cos, atan2, power, pi
mp.prec = 400
random.seed(20141017)
def b(d): return struct.unpack('<Q', struct.pack('<d', d))[0]
def f(u): return struct.unpack('<d', struct.pack('<Q', u))[0]
out = bytearray()
def rec(fn, args, res):
    out.extend(bytes([fn, len(args)]))
    for a in args: out.extend(struct.pack('<Q', b(a)))
    out.extend(struct.pack('<Q', b(res)))
N = 20000
# sin / cos
for fn, fun in ((0, sin), (1, cos)):
    for i in range(N):
        k = i % 4
        if k == 0: x = random.uniform(-40, 40)
        elif k == 1: x = random.choice((-1, 1)) * 10 ** random.uniform(-7, 6.2)
        elif k == 2:
            m = random.randint(-400000, 400000)
            x = float(m * pi / 2) + random.choice((-1, 1)) * random.randint(0, 3) * 2.0 ** random.randint(-60, -20)
        else: x = random.uniform(-8, 8)
        if x == 0.0: continue
        rec(fn, [x], float(fun(mpf(x))))
# pow
for i in range(N):
    k = i % 4
    if k == 0: x, y = random.uniform(1e-3, 4), random.uniform(-40, 40)
    elif k == 1: x, y = 0.999, float(random.randint(-6000, 6000))
    elif k == 2: x, y = 10 ** random.uniform(-5, 5), random.uniform(-3, 3)
    else: x, y = random.uniform(0.5, 2.0), float(random.randint(2, 9))
    if y == 0.0: continue
    r = power(mpf(x), mpf(y))
    if r == 0 or abs(r) < mpf(2) ** -1021 or abs(r) > mpf(2) ** 1023: continue
    rec(2, [x, y], float(r))
# atan2
for i in range(N):
    k = i % 3
    if k == 0: y, x = random.uniform(-5, 5), random.uniform(-5, 5)
    elif k == 1: y, x = random.choice((-1, 1)) * 10 ** random.uniform(-12, 12), random.choice((-1, 1)) * 10 ** random.uniform(-12, 12)
    else: y, x = random.uniform(-1, 1), random.choice((-1.0, 1.0, 2.0, 0.5))
    if x == 0.0 or y == 0.0: continue
    rec(3, [y, x], float(atan2(mpf(y), mpf(x))))
sys.stdout.buffer.write(bytes(out))
