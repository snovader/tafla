// Counts what a variant's calls allocate, on macOS, through the hook that
// libmalloc calls for every allocation of every zone, which malloc's stack
// logging and Instruments set. Swift's arrays, strings and class instances
// are allocated with malloc, as Foundation's `Data` and the libraries' own
// buffers are. Linux's glibc has no such hook, so there the harness counts
// nothing.

#if canImport(Darwin)
    import Darwin
    import Synchronization
#endif

/// What a call allocates: the allocations and reallocations, and the bytes
/// they ask for, including what Swift's runtime adds to each object, such as
/// an array's header
public struct Allocated: Sendable {
    public let allocations: Double
    public let bytes: Double
}

/// Returns what a call of `variant` allocates, over `calls` calls, the fewest
/// of three samples, or `nil` where allocations can't be counted
func allocated(by variant: Variant, calls: Int) throws -> Allocated? {
    #if canImport(Darwin)
        var fewest: Allocated?
        for _ in 0..<3 {
            let (allocations, bytes) = try MallocCounter.counting { try variant.loop(calls) }
            let sample = Allocated(
                allocations: Double(allocations) / Double(calls), bytes: Double(bytes) / Double(calls))
            if fewest == nil || sample.allocations < fewest!.allocations {
                fewest = sample
            }
        }

        return fewest
    #else
        return nil
    #endif
}

#if canImport(Darwin)
    /// libmalloc's `malloc_logger`, which it calls with each allocation's
    /// type, zone, size or pointers and result. It isn't in the SDK's
    /// headers, but libmalloc exports it, for the tools that set it.
    typealias MallocLogger = @convention(c) (UInt32, UInt, UInt, UInt, UInt, UInt32) -> Void

    enum MallocCounter {
        /// libmalloc's flags of a log's type
        static let allocate: UInt32 = 2
        static let deallocate: UInt32 = 4

        static let allocations = Atomic<Int>(0)
        static let bytes = Atomic<Int>(0)

        /// The hook's variable in libmalloc
        nonisolated(unsafe) static let logger: UnsafeMutablePointer<MallocLogger?> = {
            guard let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "malloc_logger") else {
                fatalError("libmalloc has no malloc_logger")
            }
            return symbol.assumingMemoryBound(to: MallocLogger?.self)
        }()

        /// Returns the allocations, and the bytes they ask for, that `body`
        /// makes on any thread
        static func counting(_ body: () throws -> Void) throws -> (allocations: Int, bytes: Int) {
            let logger = Self.logger
            allocations.store(0, ordering: .relaxed)
            bytes.store(0, ordering: .relaxed)

            logger.pointee = { type, _, size, reallocated, _, _ in
                guard type & MallocCounter.allocate != 0 else {
                    return
                }

                // A reallocation passes the old pointer, then the new size
                let requested = type & MallocCounter.deallocate != 0 ? reallocated : size
                MallocCounter.allocations.add(1, ordering: .relaxed)
                MallocCounter.bytes.add(Int(requested), ordering: .relaxed)
            }
            defer { logger.pointee = nil }

            try body()

            return (allocations.load(ordering: .relaxed), bytes.load(ordering: .relaxed))
        }
    }
#endif
