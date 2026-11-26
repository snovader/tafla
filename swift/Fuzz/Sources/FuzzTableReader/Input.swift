import Tafla

/// The fuzzer's input, which the checks read a byte at a time, without the
/// runtime, to find what each read of a `TableReader` should return: the
/// bytes or value that lie where the format puts them, when they lie within
/// the input, and the decode error otherwise.
struct Input {
    /// The bytes that libFuzzer passes, which it allocates at their exact
    /// length, so that AddressSanitizer reports a read past them
    let bytes: UnsafeRawBufferPointer

    var count: Int { bytes.count }

    /// The range of `byteLength` bytes at `position`, or `nil` when they
    /// don't lie within the input
    func range(at position: Int, byteLength: UInt64) -> Range<Int>? {
        // The difference can't overflow, since both are non-negative
        guard position >= 0, position <= count, byteLength <= UInt64(count - position) else {
            return nil
        }

        return position..<position + Int(byteLength)
    }

    /// The value of the little-endian bytes in `range`, of at most 8
    func littleEndian(_ range: Range<Int>) -> UInt64 {
        range.reversed().reduce(0) { value, index in value << 8 | UInt64(bytes[index]) }
    }

    /// The value of the slot as wide as `offsetType` at `position`, or `nil`
    /// when it doesn't lie within the input
    func slot(at position: Int, _ offsetType: OffsetType) -> UInt32? {
        range(at: position, byteLength: UInt64(offsetType.size)).map { UInt32(littleEndian($0)) }
    }

    /// The length or offset in the slot at `position`, or `nil` when it
    /// doesn't lie within the input, or an `Int` can't hold it, which only a
    /// 32-bit `Int` can't, and no bytes reach
    func position(at position: Int, _ offsetType: OffsetType) -> Int? {
        slot(at: position, offsetType).flatMap { Int(exactly: $0) }
    }

    /// The range of the elements of `elementSize` bytes after the count at
    /// `position`, or `nil` when the count or the elements don't lie within
    /// the input
    func array(at position: Int, _ offsetType: OffsetType, elementSize: Int) -> Range<Int>? {
        guard let count = slot(at: position, offsetType) else {
            return nil
        }

        // A byte length past a UInt64 lies past any bytes too. The count lies
        // within the input, so the elements' position doesn't overflow.
        let (byteLength, overflow) = UInt64(count).multipliedReportingOverflow(by: UInt64(elementSize))
        return overflow ? nil : range(at: position + offsetType.size, byteLength: byteLength)
    }

    /// Checks that a read returns the expected value, or fails with the
    /// decode error when there's none
    func checkResult<T: Equatable>(_ expected: T?, _ read: () throws(DecodeError) -> T) {
        do {
            let value = try read()
            require(value == expected, "Read \(value) rather than \(String(describing: expected))")
        } catch {
            checkError(error, shouldFail: expected == nil)
        }
    }

    /// Checks that `read`, which a reader of `bytes` returned, is the
    /// expected bytes, viewed in the input rather than copied
    func checkIn(_ read: RawSpan, within bytes: RawSpan, _ expected: Range<Int>?) {
        guard let expected else {
            fatalError("Read \(read.byteCount) bytes outside the input of \(count)")
        }

        let offsets = bytes.byteOffsets(of: read)
        require(offsets == expected, "Read \(String(describing: offsets)) rather than \(expected)")
    }

    /// Checks that a read failed where it should have, with an error that
    /// describes the input.
    ///
    /// Its description is left to the conformance target: most reads of
    /// arbitrary bytes fail, and formatting each error took three quarters of
    /// this target's time.
    func checkError(_ error: DecodeError, shouldFail: Bool) {
        require(shouldFail, "Failed with \(error) rather than reading")

        switch error {
        case .outOfRange(let position, let byteLength, let available):
            // A read that would leave the input
            require(
                available == count && range(at: position, byteLength: byteLength) == nil,
                "\(error) of \(count) bytes"
            )
        case .invalidUTF8(let position):
            require(position >= 0 && position < count, "\(error) of \(count) bytes")
        case .itemEndsBeforeStart(let start, let end):
            require(end < start, "\(error)")
        case .unsupportedVersion, .versionOutsideRange:
            fatalError("A read failed with \(error)")
        }
    }
}

/// Stops the fuzzer with a finding unless `condition` holds. Unlike
/// `precondition`, which only traps in an optimized build, it prints the
/// message.
func require(
    _ condition: Bool,
    _ message: @autoclosure () -> String = String(),
    file: StaticString = #file,
    line: UInt = #line
) {
    if !condition {
        fatalError(message(), file: file, line: line)
    }
}
