package io.github.snovader.tafla.benchmarks;

/**
 * A deterministic generator in [0, 1), so every run sees the same values: the TypeScript suites'
 * generator, whose values the workloads and the strings' shapes take. Its state is an {@code int},
 * as the TypeScript generator's is after its 32-bit shifts, and narrowing a {@code double} to
 * {@code float} rounds as {@code Math.fround()} does.
 */
final class Xorshift {
    private int state;

    Xorshift(int seed) {
        this.state = 0x2545f491 ^ seed;
    }

    double next() {
        state ^= state << 13;
        state ^= state >>> 17;
        state ^= state << 5;

        return Integer.toUnsignedLong(state) / 0x1p32;
    }

    /** {@return an index below {@code length}}, as {@code Math.floor(random() * length)} picks it */
    int nextIndex(int length) {
        return (int) Math.floor(next() * length);
    }
}
