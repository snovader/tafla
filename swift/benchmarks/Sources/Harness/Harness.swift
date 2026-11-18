// Times variants of a benchmark interleaved, as the other languages'
// comparisons do, rather than one after another.

/// One library's way to do a benchmark's work
public struct Variant {
    public let name: String

    /// Does the work `calls` times, passing each result to `keep(_:)`, in a
    /// loop of the variant's own, which calls the library's code directly
    public let loop: (_ calls: Int) throws -> Void

    public init(name: String, loop: @escaping (_ calls: Int) throws -> Void) {
        self.name = name
        self.loop = loop
    }
}

/// A variant's fastest and median call, and what a call allocates
public struct Measurement: Sendable {
    public let name: String
    /// The fastest round's time per call in nanoseconds, a fraction, since a
    /// call can take less than a nanosecond
    public let fastest: Double
    /// The median round's time per call, which load from other processes
    /// moves away from the fastest
    public let median: Double
    /// What a call allocates, where the harness can count it
    public let allocated: Allocated?
}

/// How long each variant runs
public struct Options: Sendable {
    public var rounds: Int
    public var warmupRounds: Int
    /// The time that one round of one variant should take
    public var sampleTime: Duration

    public init(rounds: Int, warmupRounds: Int, sampleTime: Duration) {
        self.rounds = rounds
        self.warmupRounds = warmupRounds
        self.sampleTime = sampleTime
    }
}

/// Times the variants interleaved: every round runs each of them once, in
/// alternating order, and the fastest round counts. Load from other processes
/// then slows every variant alike, and only for some rounds.
///
/// A round of a variant repeats its call for about `sampleTime`. The
/// repetitions are counted from the variant's first calls, and then again
/// from its fastest warm-up round. Then it counts what each variant's calls
/// allocate, apart from the timed rounds.
public func measure(_ variants: [Variant], options: Options) throws -> [Measurement] {
    var repetitions = try variants.map { variant in
        try calibrate(variant, sampleTime: options.sampleTime)
    }

    if options.warmupRounds > 0 {
        let warm = try rounds(of: variants, repetitions: repetitions, count: options.warmupRounds)
        for (index, samples) in warm.enumerated() {
            repetitions[index] = repetitionsOf(perCall: samples.min()!, sampleTime: options.sampleTime)
        }
    }

    let timed = try rounds(of: variants, repetitions: repetitions, count: options.rounds)

    return try variants.indices.map { index in
        let samples = timed[index].sorted()

        return Measurement(
            name: variants[index].name,
            fastest: samples[0],
            median: samples[samples.count / 2],
            allocated: try allocated(by: variants[index], calls: repetitions[index])
        )
    }
}

/// Runs rounds of the variants, each once a round, in alternating order, and
/// returns each one's time per call in every round, in nanoseconds
func rounds(of variants: [Variant], repetitions: [Int], count: Int) throws -> [[Double]] {
    var samples = [[Double]](repeating: [], count: variants.count)

    for round in 0..<count {
        for position in variants.indices {
            let index = round % 2 == 0 ? position : variants.count - 1 - position
            samples[index].append(try perCall(variants[index], calls: repetitions[index]))
        }
    }

    return samples
}

/// Returns the time per call of `calls` calls of a variant, in nanoseconds.
/// Swift frees what a call allocated when its last reference goes, within the
/// variant's own round, so no collection of another variant's garbage
/// slows it.
func perCall(_ variant: Variant, calls: Int) throws -> Double {
    let clock = ContinuousClock()

    let start = clock.now
    try variant.loop(calls)
    let elapsed = start.duration(to: clock.now)

    return nanoseconds(elapsed) / Double(calls)
}

/// Counts the repetitions that take about `sampleTime`, from calls that take
/// at least a tenth of it
func calibrate(_ variant: Variant, sampleTime: Duration) throws -> Int {
    let clock = ContinuousClock()

    var calls = 1
    while true {
        let start = clock.now
        try variant.loop(calls)
        let elapsed = start.duration(to: clock.now)
        if elapsed >= sampleTime / 10 {
            return repetitionsOf(perCall: nanoseconds(elapsed) / Double(calls), sampleTime: sampleTime)
        }

        calls *= 2
    }
}

/// Returns the calls of `perCall` nanoseconds that take about `sampleTime`
func repetitionsOf(perCall: Double, sampleTime: Duration) -> Int {
    max(1, Int(nanoseconds(sampleTime) / max(perCall, 1)))
}

func nanoseconds(_ duration: Duration) -> Double {
    let (seconds, attoseconds) = duration.components
    return Double(seconds) * 1e9 + Double(attoseconds) / 1e9
}

/// Keeps a result alive: a call that the optimizer can neither see into nor
/// leave out, so that it can't leave out the work that made the result, as
/// package-benchmark's `blackHole(_:)` does
@inline(never)
@_optimize(none)
public func keep<T>(_ value: T) {}
