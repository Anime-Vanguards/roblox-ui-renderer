"""Roblox's randomness, reference implementation checked against Studio.

    python tools/analysis/anim_random.py

math.random and the Random datatype are both Luau's PCG32 (lmathlib.cpp): math.randomseed(n) and
Random.new(n) seed it the same way, math.random() / Random:NextNumber() build a double from two
draws, math.random(l, u) / Random:NextInteger(l, u) scale one draw (two for ranges of 2^32 or more).
The values below were read in Studio (2026-09-24) and are reproduced exactly; Lune's
math.random gives the same stream, so seeding both sides (rur compare seeds math.random with 1 right
before the build, runtime/engine/scheduler.luau does the same offline) makes them draw alike.
"""

import math

MASK64 = (1 << 64) - 1


class Pcg32:
    def __init__(self, seed):
        # Random.new(seed): int64 two's complement (-3 -> 2^64 - 3), fractional seeds truncate
        self.state = 0
        self.draw()
        self.state = (self.state + (int(seed) & MASK64)) & MASK64
        self.draw()

    def draw(self):
        old = self.state
        self.state = (old * 6364136223846793005 + (105 | 1)) & MASK64
        xorshifted = (((old >> 18) ^ old) >> 27) & 0xFFFFFFFF
        rot = old >> 59
        return ((xorshifted >> rot) | (xorshifted << ((-rot) & 31))) & 0xFFFFFFFF

    # math.random() / Random:NextNumber()
    def number(self, low=None, high=None):
        lo = self.draw()
        hi = self.draw()
        unit = math.ldexp(float(lo | (hi << 32)), -64)
        return unit if low is None else low + (high - low) * unit

    # math.random(l, u) / Random:NextInteger(l, u)
    def integer(self, low, high):
        span = high - low + 1
        if span < (1 << 32):
            return low + ((span * self.draw()) >> 32)
        lo = self.draw()
        hi = self.draw()
        return low + (((lo | (hi << 32)) * span) >> 64)


# (description, value in Studio, value here)
def checks():
    results = []
    stream = Pcg32(42)
    results.append(("math.randomseed(42) random()", 0.22072993115387124, stream.number()))
    results.append(("random(1, 100)", 5, stream.integer(1, 100)))
    results.append(("random(5)", 5, stream.integer(1, 5)))
    results.append(("random()", 0.76355000807415585, stream.number()))
    generator = Pcg32(7)
    results.append(("Random.new(7):NextNumber()", 0.87370859248613331, generator.number()))
    results.append(("NextInteger(1, 100)", 2, generator.integer(1, 100)))
    results.append(("NextNumber(5, 10)", 5.8048382429780405, generator.number(5, 10)))
    results.append(("NextInteger(-1000000, 1000000)", -382016, generator.integer(-1000000, 1000000)))
    results.append(("NextInteger(0, 4294967295)", 462466561, generator.integer(0, 4294967295)))
    results.append(("NextInteger(0, 2^40)", 360504993464, generator.integer(0, 1 << 40)))
    results.append(("NextNumber()", 0.20078177722661195, generator.number()))
    results.append(("Random.new(-3):NextNumber()", 0.23554798859579634, Pcg32(-3).number()))
    results.append(("Random.new(2.7):NextNumber()", 0.27759919461205307, Pcg32(2.7).number()))
    return results


def main():
    exact = 0
    for description, studio, ours in checks():
        same = studio == ours
        exact += same
        print(f"{'ok  ' if same else 'DIFF'} {description}: Studio {studio!r}, here {ours!r}")
    print(f"{exact} of {len(checks())} exact")


if __name__ == "__main__":
    main()
